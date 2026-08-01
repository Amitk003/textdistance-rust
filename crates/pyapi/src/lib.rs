use pyo3::prelude::*;
use pyo3::types::{PyAny, PyAnyMethods, PyList, PyListMethods, PyString, PyTuple};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};

use tdcore::edit;
use tdcore::simple;

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

fn seq_to_chars(seq: &Bound<'_, PyAny>) -> PyResult<Vec<char>> {
    let s: String = seq.extract()?;
    Ok(s.chars().collect())
}

type BoolPred<'a> = Box<dyn Fn(&Py<PyAny>, &Py<PyAny>) -> bool + 'a>;
type ColPred<'a> = Box<dyn Fn(&[Option<&Py<PyAny>>]) -> bool + 'a>;
type FloatSim<'a> = Box<dyn Fn(&Py<PyAny>, &Py<PyAny>) -> f64 + 'a>;

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
        let mut char_seqs: Vec<Vec<char>> = Vec::new();
        for item in sequences.try_iter()? {
            let seq = item?;
            if seq.is_instance_of::<PyString>() {
                char_seqs.push(seq_to_chars(&seq)?);
            } else {
                all_str = false;
                break;
            }
        }
        if all_str {
            let test = |col: &[Option<&char>]| -> bool {
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let eq = |x: &char, y: &char| x == y;
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
            let a = seq_to_chars(s1)?;
            let b = seq_to_chars(s2)?;
            let eq = |x: &char, y: &char| x == y;
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let eq = |x: &char, y: &char| x == y;
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let eq = |x: &char, y: &char| x == y;
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
    let a = seq_to_chars(s1)?;
    let b = seq_to_chars(s2)?;
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let test = |col: &[Option<&char>]| -> bool {
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let sim = |x: &char, y: &char| if x == y { 1.0 } else { 0.0 };
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let sim = |x: &char, y: &char| if x == y { 1.0 } else { 0.0 };
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
        let a = seq_to_chars(s1)?;
        let b = seq_to_chars(s2)?;
        let sim = |x: &char, y: &char| if x == y { 1.0 } else { 0.0 };
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

/// Collect a Python sequence of single-character strings into chars,
/// silently ignoring entries that are not single characters (upstream group
/// sets can contain such entries but they can never match a single char).
fn chars_list(obj: &Bound<'_, PyAny>) -> PyResult<Vec<char>> {
    let mut out = Vec::new();
    for item in obj.try_iter()? {
        let s: String = item?.extract()?;
        let mut chars = s.chars();
        if let Some(c) = chars.next() {
            if chars.next().is_none() {
                out.push(c);
            }
        }
    }
    Ok(out)
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
    let a = seq_to_chars(s1)?;
    let b = seq_to_chars(s2)?;
    let mut group_vecs: Vec<Vec<char>> = Vec::new();
    for item in groups.try_iter()? {
        group_vecs.push(chars_list(&item?)?);
    }
    let group_refs: Vec<&[char]> = group_vecs.iter().map(|g| g.as_slice()).collect();
    let ungrouped_vec = chars_list(ungrouped)?;
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
        let mut char_seqs: Vec<Vec<char>> = Vec::new();
        for item in sequences.try_iter()? {
            let seq = item?;
            if seq.is_instance_of::<PyString>() {
                char_seqs.push(seq_to_chars(&seq)?);
            } else {
                all_str = false;
                break;
            }
        }
        if all_str {
            let test = |x: &char, y: &char| x == y;
            let result = simple::common_prefix(&char_seqs, test);
            let list = PyList::empty(py);
            for c in result {
                list.append(PyString::new(py, &c.to_string()).into_any())?;
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
    Ok(())
}
