use num_bigint::BigInt;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyAnyMethods, PyList, PyListMethods, PyString, PyTuple};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use tdcore::compression;
use tdcore::edit;
use tdcore::sequence;
use tdcore::simple;
use tdcore::token;

/// Collects the first Python error raised inside a comparison callback so the
/// kernel (which returns plain values) can stay pure while the error still
/// propagates to the caller after the kernel returns.
#[derive(Default)]
struct ErrSlot(RefCell<Option<PyErr>>);

impl ErrSlot {
    fn record(&self, err: PyErr) {
        if self.0.borrow().is_none() {
            *self.0.borrow_mut() = Some(err);
        }
    }
    fn take(&self) -> PyResult<()> {
        match self.0.borrow_mut().take() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

/// Extract a Python str as a sequence of Unicode code points, including lone
/// surrogates, which is how Python itself models strings (surrogates are
/// valid one-element code points). Fast path is UTF-8 extraction; when that
/// fails (a lone surrogate is present), the code points are taken through
/// Python semantics via `list(map(ord, data))`.
fn seq_to_codepoints(seq: &Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    if let Ok(s) = seq.extract::<String>() {
        return Ok(s.chars().map(|c| c as u32).collect());
    }
    let py = seq.py();
    let builtins = py.import("builtins")?;
    let ord_ = builtins.getattr("ord")?;
    let mapped = builtins.getattr("map")?.call1((ord_, seq))?;
    let as_list = builtins.getattr("list")?.call1((mapped,))?;
    as_list.extract::<Vec<u32>>()
}

/// Require a single-character Python str and return its code point.
fn single_codepoint(value: &Bound<'_, PyAny>) -> PyResult<u32> {
    match seq_to_codepoints(value)?.as_slice() {
        [cp] => Ok(*cp),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "terminator must be a single character",
        )),
    }
}

type BoolPred<'a> = Box<dyn Fn(&Py<PyAny>, &Py<PyAny>) -> bool + 'a>;
type ColPred<'a> = Box<dyn Fn(&[Option<&Py<PyAny>>]) -> bool + 'a>;
type FloatSim<'a> = Box<dyn Fn(&Py<PyAny>, &Py<PyAny>) -> f64 + 'a>;
type HashKeyPred<'a> = Box<dyn Fn(&PyHashKey, &PyHashKey) -> bool + 'a>;

fn seq_to_objects(seq: &Bound<'_, PyAny>) -> PyResult<Vec<Py<PyAny>>> {
    let mut out = Vec::new();
    for item in seq.try_iter()? {
        out.push(item?.unbind());
    }
    Ok(out)
}

/// Python value as a hashable map key for the unrestricted Damerau-Levenshtein
/// recurrence. Hashing uses Python's own hash so behavior matches the original
/// dict, and equality uses Python `==`, so hash collisions are resolved the
/// same way Python resolves them.
struct PyHashKey {
    obj: Py<PyAny>,
    hash: u64,
}

impl Clone for PyHashKey {
    fn clone(&self) -> Self {
        let obj = Python::try_attach(|py| self.obj.clone_ref(py))
            .expect("PyHashKey::clone requires an attached interpreter");
        PyHashKey {
            obj,
            hash: self.hash,
        }
    }
}

impl PartialEq for PyHashKey {
    fn eq(&self, other: &Self) -> bool {
        Python::try_attach(|py| self.obj.bind(py).eq(other.obj.bind(py)).unwrap_or(false))
            .unwrap_or(false)
    }
}

impl Eq for PyHashKey {}

impl Hash for PyHashKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

fn objects_to_hashkeys<'py>(py: Python<'py>, objs: Vec<Py<PyAny>>) -> PyResult<Vec<PyHashKey>> {
    let mut out = Vec::with_capacity(objs.len());
    for obj in objs {
        let hash = obj.bind(py).hash()? as u64;
        out.push(PyHashKey { obj, hash });
    }
    Ok(out)
}

/// Binary predicate over arbitrary Python objects, using `test_func` if given
/// (truthiness of its return value) or Python equality otherwise.
fn bool_pred<'a, 'py>(
    py: Python<'py>,
    slot: &'a ErrSlot,
    test_func: &Option<Bound<'py, PyAny>>,
) -> BoolPred<'a>
where
    'py: 'a,
{
    match test_func {
        Some(tf) => {
            let tf = tf.clone();
            Box::new(
                move |a: &Py<PyAny>, b: &Py<PyAny>| match tf.call1((a.bind(py), b.bind(py))) {
                    Ok(v) => v.is_truthy().unwrap_or(false),
                    Err(e) => {
                        slot.record(e);
                        false
                    }
                },
            )
        }
        None => Box::new(
            move |a: &Py<PyAny>, b: &Py<PyAny>| match a.bind(py).eq(b.bind(py)) {
                Ok(v) => v,
                Err(e) => {
                    slot.record(e);
                    false
                }
            },
        ),
    }
}

/// Column predicate for the N-sequence Hamming loop.
fn col_pred<'a, 'py>(
    py: Python<'py>,
    slot: &'a ErrSlot,
    test_func: &Option<Bound<'py, PyAny>>,
) -> ColPred<'a>
where
    'py: 'a,
{
    match test_func {
        Some(tf) => {
            let tf = tf.clone();
            Box::new(move |col: &[Option<&Py<PyAny>>]| {
                let args: Vec<Bound<'py, PyAny>> = col
                    .iter()
                    .map(|o| match o {
                        Some(b) => b.bind(py).clone(),
                        None => py.None().bind(py).clone(),
                    })
                    .collect();
                let tuple = match PyTuple::new(py, args) {
                    Ok(t) => t,
                    Err(e) => {
                        slot.record(e);
                        return false;
                    }
                };
                match tf.call1(&tuple) {
                    Ok(v) => v.is_truthy().unwrap_or(false),
                    Err(e) => {
                        slot.record(e);
                        false
                    }
                }
            })
        }
        None => Box::new(move |col: &[Option<&Py<PyAny>>]| {
            let Some(first) = col.first().and_then(|o| o.as_ref()) else {
                return false;
            };
            for o in col.iter().skip(1) {
                let ok = match o {
                    Some(other) => match first.bind(py).eq(other.bind(py)) {
                        Ok(v) => v,
                        Err(e) => {
                            slot.record(e);
                            false
                        }
                    },
                    None => false,
                };
                if !ok {
                    return false;
                }
            }
            true
        }),
    }
}

/// Float similarity predicate, for the alignment kernels.
fn float_sim<'a, 'py>(
    py: Python<'py>,
    slot: &'a ErrSlot,
    sim_func: &Option<Bound<'py, PyAny>>,
) -> FloatSim<'a>
where
    'py: 'a,
{
    match sim_func {
        Some(f) => {
            let f = f.clone();
            Box::new(
                move |a: &Py<PyAny>, b: &Py<PyAny>| match f.call1((a.bind(py), b.bind(py))) {
                    Ok(v) => v.extract::<f64>().unwrap_or_else(|e| {
                        slot.record(e);
                        0.0
                    }),
                    Err(e) => {
                        slot.record(e);
                        0.0
                    }
                },
            )
        }
        None => Box::new(
            move |a: &Py<PyAny>, b: &Py<PyAny>| match a.bind(py).eq(b.bind(py)) {
                Ok(true) => 1.0,
                Ok(false) => 0.0,
                Err(e) => {
                    slot.record(e);
                    0.0
                }
            },
        ),
    }
}

#[pyfunction]
#[pyo3(signature = (sequences, truncate=false, test_func=None))]
fn hamming<'py>(
    py: Python<'py>,
    sequences: &Bound<'py, PyAny>,
    truncate: bool,
    test_func: Option<Bound<'py, PyAny>>,
) -> PyResult<usize> {
    if test_func.is_none() {
        let mut all_str = true;
        let mut char_seqs: Vec<Vec<u32>> = Vec::new();
        for item in sequences.try_iter()? {
            let seq = item?;
            if seq.is_instance_of::<PyString>() {
                char_seqs.push(seq_to_codepoints(&seq)?);
            } else {
                all_str = false;
                break;
            }
        }
        if all_str {
            let test = |col: &[Option<&u32>]| -> bool {
                match col.first().and_then(|o| o.as_ref()) {
                    Some(first) => col.iter().all(|o| match o {
                        Some(c) => c == first,
                        None => false,
                    }),
                    None => false,
                }
            };
            return Ok(edit::hamming_distance(&char_seqs, truncate, test));
        }
    }
    let mut obj_seqs: Vec<Vec<Py<PyAny>>> = Vec::new();
    for item in sequences.try_iter()? {
        obj_seqs.push(seq_to_objects(&item?)?);
    }
    let slot = ErrSlot::default();
    let test = col_pred(py, &slot, &test_func);
    let result = edit::hamming_distance(&obj_seqs, truncate, &test);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, test_func=None))]
fn levenshtein<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    test_func: Option<Bound<'py, PyAny>>,
) -> PyResult<usize> {
    if test_func.is_none() && s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let eq = |x: &u32, y: &u32| x == y;
        return Ok(edit::levenshtein(&a, &b, eq));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let pred = bool_pred(py, &slot, &test_func);
    let result = edit::levenshtein(&a, &b, &pred);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, restricted=true, test_func=None))]
fn damerau_levenshtein<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    restricted: bool,
    test_func: Option<Bound<'py, PyAny>>,
) -> PyResult<usize> {
    if restricted {
        if test_func.is_none() && s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>()
        {
            let a = seq_to_codepoints(s1)?;
            let b = seq_to_codepoints(s2)?;
            let eq = |x: &u32, y: &u32| x == y;
            return Ok(edit::damerau_levenshtein_restricted(&a, &b, eq));
        }
        let a = seq_to_objects(s1)?;
        let b = seq_to_objects(s2)?;
        let slot = ErrSlot::default();
        let pred = bool_pred(py, &slot, &test_func);
        let result = edit::damerau_levenshtein_restricted(&a, &b, &pred);
        slot.take()?;
        Ok(result)
    } else if test_func.is_none()
        && s1.is_instance_of::<PyString>()
        && s2.is_instance_of::<PyString>()
    {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let eq = |x: &u32, y: &u32| x == y;
        Ok(edit::damerau_levenshtein_unrestricted(&a, &b, eq))
    } else {
        let a = objects_to_hashkeys(py, seq_to_objects(s1)?)?;
        let b = objects_to_hashkeys(py, seq_to_objects(s2)?)?;
        let slot = ErrSlot::default();
        let slot_ref = &slot;
        let pred = match &test_func {
            Some(tf) => {
                let tf = tf.clone();
                Box::new(move |x: &PyHashKey, y: &PyHashKey| {
                    match tf.call1((x.obj.bind(py), y.obj.bind(py))) {
                        Ok(v) => v.is_truthy().unwrap_or(false),
                        Err(e) => {
                            slot_ref.record(e);
                            false
                        }
                    }
                }) as Box<dyn Fn(&PyHashKey, &PyHashKey) -> bool + '_>
            }
            None => Box::new(move |x: &PyHashKey, y: &PyHashKey| {
                match x.obj.bind(py).eq(y.obj.bind(py)) {
                    Ok(v) => v,
                    Err(e) => {
                        slot_ref.record(e);
                        false
                    }
                }
            }),
        };
        let result = edit::damerau_levenshtein_unrestricted(&a, &b, &pred);
        slot.take()?;
        Ok(result)
    }
}

#[pyfunction]
#[pyo3(signature = (s1, s2, prefix_weight=0.1, long_tolerance=false, winklerize=true))]
fn jaro_winkler<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    prefix_weight: f64,
    long_tolerance: bool,
    winklerize: bool,
) -> PyResult<f64> {
    if s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let eq = |x: &u32, y: &u32| x == y;
        return Ok(edit::jaro_winkler(
            &a,
            &b,
            prefix_weight,
            long_tolerance,
            winklerize,
            eq,
        ));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let pred = bool_pred(py, &slot, &None);
    let result = edit::jaro_winkler(&a, &b, prefix_weight, long_tolerance, winklerize, &pred);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, long_strings=false))]
fn strcmp95(s1: &Bound<'_, PyAny>, s2: &Bound<'_, PyAny>, long_strings: bool) -> PyResult<f64> {
    let a = seq_to_codepoints(s1)?;
    let b = seq_to_codepoints(s2)?;
    Ok(edit::strcmp95(&a, &b, long_strings))
}

#[pyfunction]
#[pyo3(signature = (s1, s2, threshold=0.25, maxmismatches=2))]
fn mlipns<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    threshold: f64,
    maxmismatches: usize,
) -> PyResult<f64> {
    if s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let test = |col: &[Option<&u32>]| -> bool {
            match col.first().and_then(|o| o.as_ref()) {
                Some(first) => col.iter().all(|o| match o {
                    Some(c) => c == first,
                    None => false,
                }),
                None => false,
            }
        };
        return Ok(edit::mlipns(&a, &b, threshold, maxmismatches, test));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let test = col_pred(py, &slot, &None);
    let result = edit::mlipns(&a, &b, threshold, maxmismatches, &test);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, gap_cost=1.0, sim_func=None))]
fn needleman_wunsch<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    gap_cost: f64,
    sim_func: Option<Bound<'py, PyAny>>,
) -> PyResult<f64> {
    if sim_func.is_none() && s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let sim = |x: &u32, y: &u32| if x == y { 1.0 } else { 0.0 };
        return Ok(edit::needleman_wunsch(&a, &b, gap_cost, sim));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let sim = float_sim(py, &slot, &sim_func);
    let result = edit::needleman_wunsch(&a, &b, gap_cost, &sim);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, gap_cost=1.0, sim_func=None))]
fn smith_waterman<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    gap_cost: f64,
    sim_func: Option<Bound<'py, PyAny>>,
) -> PyResult<f64> {
    if sim_func.is_none() && s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let sim = |x: &u32, y: &u32| if x == y { 1.0 } else { 0.0 };
        return Ok(edit::smith_waterman(&a, &b, gap_cost, sim));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let sim = float_sim(py, &slot, &sim_func);
    let result = edit::smith_waterman(&a, &b, gap_cost, &sim);
    slot.take()?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (s1, s2, gap_open=1.0, gap_ext=0.4, sim_func=None))]
fn gotoh<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
    gap_open: f64,
    gap_ext: f64,
    sim_func: Option<Bound<'py, PyAny>>,
) -> PyResult<f64> {
    if sim_func.is_none() && s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let sim = |x: &u32, y: &u32| if x == y { 1.0 } else { 0.0 };
        return Ok(edit::gotoh(&a, &b, gap_open, gap_ext, sim));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let sim = float_sim(py, &slot, &sim_func);
    let result = edit::gotoh(&a, &b, gap_open, gap_ext, &sim);
    slot.take()?;
    Ok(result)
}

/// Turn a single Unicode code point back into a one-character Python str,
/// including lone surrogates (which Rust cannot represent as a `char`).
fn codepoint_to_pystr<'py>(py: Python<'py>, cp: u32) -> PyResult<Bound<'py, PyString>> {
    let builtins = py.import("builtins")?;
    let chr = builtins.getattr("chr")?;
    let obj = chr.call1((cp,))?;
    Ok(obj.cast_into::<PyString>()?)
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (s1, s2, match_cost, group_cost, mismatch_cost, local, groups, ungrouped, max_length))]
fn editex(
    s1: &Bound<'_, PyAny>,
    s2: &Bound<'_, PyAny>,
    match_cost: i64,
    group_cost: i64,
    mismatch_cost: i64,
    local: bool,
    groups: &Bound<'_, PyAny>,
    ungrouped: &Bound<'_, PyAny>,
    max_length: i64,
) -> PyResult<i64> {
    let a = seq_to_codepoints(s1)?;
    let b = seq_to_codepoints(s2)?;
    let mut group_vecs: Vec<Vec<u32>> = Vec::new();
    for item in groups.try_iter()? {
        group_vecs.push(seq_to_codepoints(&item?)?);
    }
    let group_refs: Vec<&[u32]> = group_vecs.iter().map(|g| g.as_slice()).collect();
    let ungrouped_vec = seq_to_codepoints(ungrouped)?;
    Ok(edit::editex(
        &a,
        &b,
        match_cost,
        group_cost,
        mismatch_cost,
        local,
        &group_refs,
        &ungrouped_vec,
        max_length,
    ))
}

#[pyfunction]
#[pyo3(signature = (sequences, sim_test=None))]
fn common_prefix<'py>(
    py: Python<'py>,
    sequences: &Bound<'py, PyAny>,
    sim_test: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    if sim_test.is_none() {
        let mut all_str = true;
        let mut char_seqs: Vec<Vec<u32>> = Vec::new();
        for item in sequences.try_iter()? {
            let seq = item?;
            if seq.is_instance_of::<PyString>() {
                char_seqs.push(seq_to_codepoints(&seq)?);
            } else {
                all_str = false;
                break;
            }
        }
        if all_str {
            let test = |x: &u32, y: &u32| x == y;
            let result = simple::common_prefix(&char_seqs, test);
            let list = PyList::empty(py);
            for c in result {
                list.append(codepoint_to_pystr(py, *c)?.into_any())?;
            }
            return Ok(list.into_any());
        }
    }
    let mut obj_seqs: Vec<Vec<Py<PyAny>>> = Vec::new();
    for item in sequences.try_iter()? {
        obj_seqs.push(seq_to_objects(&item?)?);
    }
    let slot = ErrSlot::default();
    let pred = bool_pred(py, &slot, &sim_test);
    let result = simple::common_prefix(&obj_seqs, &pred);
    slot.take()?;
    let list = PyList::empty(py);
    for obj in result {
        list.append(obj.bind(py))?;
    }
    Ok(list.into_any())
}

#[pyfunction]
fn length(sequences: &Bound<'_, PyAny>) -> PyResult<usize> {
    let mut lengths = Vec::new();
    for item in sequences.try_iter()? {
        lengths.push(item?.len()?);
    }
    Ok(simple::length_distance(&lengths))
}

/// Matched indices (into `s1`, increasing order) for the longest common
/// subsequence, mirroring the `LCSSeq._dynamic` matrix walk.
#[pyfunction]
fn lcsseq<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
) -> PyResult<Vec<usize>> {
    if s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let eq = |x: &u32, y: &u32| x == y;
        return Ok(sequence::lcsseq(&a, &b, eq));
    }
    let a = seq_to_objects(s1)?;
    let b = seq_to_objects(s2)?;
    let slot = ErrSlot::default();
    let pred = bool_pred(py, &slot, &None);
    let result = sequence::lcsseq(&a, &b, &pred);
    slot.take()?;
    Ok(result)
}

/// `(besti, bestsize)` from the difflib-standard longest common substring,
/// mirroring `LCSStr._standart`.
#[pyfunction]
fn lcsstr_standard<'py>(
    py: Python<'py>,
    s1: &Bound<'py, PyAny>,
    s2: &Bound<'py, PyAny>,
) -> PyResult<(usize, usize)> {
    if s1.is_instance_of::<PyString>() && s2.is_instance_of::<PyString>() {
        let a = seq_to_codepoints(s1)?;
        let b = seq_to_codepoints(s2)?;
        let eq = |x: &u32, y: &u32| x == y;
        return Ok(sequence::lcsstr_standard(&a, &b, eq));
    }
    let a = objects_to_hashkeys(py, seq_to_objects(s1)?)?;
    let b = objects_to_hashkeys(py, seq_to_objects(s2)?)?;
    let slot = ErrSlot::default();
    let slot_ref = &slot;
    let pred: HashKeyPred<'_> =
        Box::new(
            move |x: &PyHashKey, y: &PyHashKey| match x.obj.bind(py).eq(y.obj.bind(py)) {
                Ok(v) => v,
                Err(e) => {
                    slot_ref.record(e);
                    false
                }
            },
        );
    let result = sequence::lcsstr_standard(&a, &b, &pred);
    slot.take()?;
    Ok(result)
}

/// Sum of matched block lengths for the Ratcliff-Obershelp recursion,
/// mirroring `RatcliffObershelp._find`.
#[pyfunction]
fn ratcliff_obershelp_find(sequences: &Bound<'_, PyAny>) -> PyResult<usize> {
    let mut seqs: Vec<Vec<u32>> = Vec::new();
    for item in sequences.try_iter()? {
        seqs.push(seq_to_codepoints(&item?)?);
    }
    Ok(sequence::ratcliff_obershelp(&seqs))
}

/// Counter statistics shared by the token-family algorithms.
///
/// `counters` is a list of dict-like objects (Counters), `as_set` mirrors the
/// adapter's `_count_counters` mode. Returns `(intersection, union, counts)`.
#[pyfunction]
fn token_stats(counters: &Bound<'_, PyAny>, as_set: bool) -> PyResult<(f64, f64, Vec<f64>)> {
    let mut maps: Vec<HashMap<PyHashKey, f64>> = Vec::new();
    for counter in counters.try_iter()? {
        let counter = counter?;
        let items = counter.call_method0("items")?;
        let mut map: HashMap<PyHashKey, f64> = HashMap::new();
        for item in items.try_iter()? {
            let item = item?;
            let key = item.get_item(0)?;
            let count = item.get_item(1)?;
            let hash = key.hash()? as u64;
            map.insert(
                PyHashKey {
                    obj: key.unbind(),
                    hash,
                },
                count.extract()?,
            );
        }
        maps.push(map);
    }
    let stats = token::token_stats(&maps, as_set);
    Ok((stats.intersection, stats.union, stats.counts))
}

/// Arithmetic coding of a string, returning the exact encoded fraction as
/// `(numerator, denominator)`, mirroring `ArithNCD._compress`.
#[pyfunction]
fn arith_compress(
    data: &Bound<'_, PyAny>,
    terminator: Option<&Bound<'_, PyAny>>,
) -> PyResult<(BigInt, BigInt)> {
    let cp = seq_to_codepoints(data)?;
    let term = match terminator {
        None => None,
        Some(t) => Some(single_codepoint(t)?),
    };
    Ok(compression::arith_compress(&cp, term))
}

/// Run-length encoding over characters, mirroring `RLENCD._compress`.
/// Returns the encoded code points; the adapter joins them into a str.
#[pyfunction]
fn rle(data: &Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    let cp = seq_to_codepoints(data)?;
    Ok(compression::rle(&cp))
}

/// Burrows-Wheeler transform over characters, mirroring `BWTRLENCD._compress`.
/// Returns the transformed code points; the adapter joins them into a str.
#[pyfunction]
fn bwt(data: &Bound<'_, PyAny>, terminator: &Bound<'_, PyAny>) -> PyResult<Vec<u32>> {
    let cp = seq_to_codepoints(data)?;
    let term = single_codepoint(terminator)?;
    Ok(compression::bwt(&cp, term))
}

/// Sum of square roots of per-element counts, mirroring `SqrtNCD._get_size`.
#[pyfunction]
fn sqrt_size(data: &Bound<'_, PyAny>) -> PyResult<f64> {
    let cp = seq_to_codepoints(data)?;
    Ok(compression::sqrt_size(&cp))
}

/// Shannon entropy of per-element counts, mirroring `EntropyNCD._compress`.
#[pyfunction]
fn entropy(data: &Bound<'_, PyAny>, base: f64) -> PyResult<f64> {
    let cp = seq_to_codepoints(data)?;
    Ok(compression::entropy(&cp, base))
}

/// bzip2-compressed bytes with the 15-byte header dropped, mirroring
/// `BZ2NCD._compress`.
#[pyfunction]
fn bz2_compress(data: &[u8]) -> Vec<u8> {
    codec::bz2_compress(data)
}

/// zlib-compressed bytes with the 2-byte header dropped, mirroring
/// `ZLIBNCD._compress`.
#[pyfunction]
fn zlib_compress(data: &[u8]) -> Vec<u8> {
    codec::zlib_compress(data)
}

/// lzma/xz-compressed bytes with the 14-byte header dropped, mirroring
/// `LZMANCD._compress`.
#[pyfunction]
fn lzma_compress(data: &[u8]) -> Vec<u8> {
    codec::lzma_compress(data)
}

/// Extension module `textdistance._textdistance`.
///
/// Kernel functions are registered here as they are ported. The Python
/// adapter in `python/textdistance/` calls into this module and never
/// reimplements algorithm math.
#[pymodule]
fn _textdistance(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(hamming, m)?)?;
    m.add_function(wrap_pyfunction!(levenshtein, m)?)?;
    m.add_function(wrap_pyfunction!(damerau_levenshtein, m)?)?;
    m.add_function(wrap_pyfunction!(jaro_winkler, m)?)?;
    m.add_function(wrap_pyfunction!(strcmp95, m)?)?;
    m.add_function(wrap_pyfunction!(mlipns, m)?)?;
    m.add_function(wrap_pyfunction!(needleman_wunsch, m)?)?;
    m.add_function(wrap_pyfunction!(smith_waterman, m)?)?;
    m.add_function(wrap_pyfunction!(gotoh, m)?)?;
    m.add_function(wrap_pyfunction!(editex, m)?)?;
    m.add_function(wrap_pyfunction!(common_prefix, m)?)?;
    m.add_function(wrap_pyfunction!(length, m)?)?;
    m.add_function(wrap_pyfunction!(lcsseq, m)?)?;
    m.add_function(wrap_pyfunction!(lcsstr_standard, m)?)?;
    m.add_function(wrap_pyfunction!(ratcliff_obershelp_find, m)?)?;
    m.add_function(wrap_pyfunction!(token_stats, m)?)?;
    m.add_function(wrap_pyfunction!(arith_compress, m)?)?;
    m.add_function(wrap_pyfunction!(rle, m)?)?;
    m.add_function(wrap_pyfunction!(bwt, m)?)?;
    m.add_function(wrap_pyfunction!(sqrt_size, m)?)?;
    m.add_function(wrap_pyfunction!(entropy, m)?)?;
    m.add_function(wrap_pyfunction!(bz2_compress, m)?)?;
    m.add_function(wrap_pyfunction!(zlib_compress, m)?)?;
    m.add_function(wrap_pyfunction!(lzma_compress, m)?)?;
    Ok(())
}
