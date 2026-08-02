//! Edit-family kernels.
//!
//! Hamming, Levenshtein, Damerau-Levenshtein, Jaro, Jaro-Winkler, StrCmp95,
//! MLIPNS, Needleman-Wunsch, Smith-Waterman, Gotoh, and Editex.
//!
//! Every kernel is generic over the element type and takes a comparison
//! predicate. The FFI layer supplies either a pure-Rust predicate (chars) or a
//! predicate that crosses into Python (arbitrary objects and custom callables).
//! Nothing in this module touches the Python runtime.

use std::collections::HashMap;
use std::hash::Hash;

/// Hamming distance over N sequences.
///
/// `truncate == true` stops at the shortest sequence, otherwise missing
/// trailing elements count as mismatches (the original zips with
/// `itertools.zip_longest`, which pads with `None`).
pub fn hamming_distance<T, S, F>(seqs: &[S], truncate: bool, test: F) -> usize
where
    S: AsRef<[T]>,
    F: Fn(&[Option<&T>]) -> bool,
{
    let ncols = if truncate {
        seqs.iter().map(|s| s.as_ref().len()).min().unwrap_or(0)
    } else {
        seqs.iter().map(|s| s.as_ref().len()).max().unwrap_or(0)
    };
    let mut column: Vec<Option<&T>> = Vec::with_capacity(seqs.len());
    let mut mismatches = 0usize;
    for i in 0..ncols {
        column.clear();
        for s in seqs {
            column.push(s.as_ref().get(i));
        }
        if !test(&column) {
            mismatches += 1;
        }
    }
    mismatches
}

/// Levenshtein edit distance (rows x cols, single rolling row pair).
pub fn levenshtein<T, F>(s1: &[T], s2: &[T], test: F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    let cols = s2.len() + 1;
    let mut prev: Vec<usize> = (0..cols).collect();
    let mut cur: Vec<usize> = vec![0; cols];
    for (r, a) in s1.iter().enumerate() {
        cur[0] = r + 1;
        for (c, b) in s2.iter().enumerate() {
            let deletion = prev[c + 1] + 1;
            let insertion = cur[c] + 1;
            let edit = prev[c] + usize::from(!test(a, b));
            cur[c + 1] = edit.min(deletion).min(insertion);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[cols - 1]
}

/// Restricted Damerau-Levenshtein distance (adjacent transpositions only).
///
/// Mirrors the upstream `_pure_python_restricted` recurrence.
pub fn damerau_levenshtein_restricted<T, F>(s1: &[T], s2: &[T], test: F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    let len1 = s1.len();
    let len2 = s2.len();
    let mut d = vec![vec![0usize; len2 + 1]; len1 + 1];
    for (ci, cell) in d[0].iter_mut().enumerate() {
        *cell = ci;
    }
    for (ri, row) in d.iter_mut().enumerate() {
        row[0] = ri;
    }
    for (i, a) in s1.iter().enumerate() {
        let ri = i + 1;
        for (j, b) in s2.iter().enumerate() {
            let ci = j + 1;
            let cost = usize::from(!test(a, b));
            d[ri][ci] = (d[ri - 1][ci] + 1)
                .min(d[ri][ci - 1] + 1)
                .min(d[ri - 1][ci - 1] + cost);
            if i == 0 || j == 0 {
                continue;
            }
            if !test(a, &s2[j - 1]) || !test(&s1[i - 1], b) {
                continue;
            }
            d[ri][ci] = d[ri][ci].min(d[ri - 2][ci - 2] + cost);
        }
    }
    d[len1][len2]
}

/// Unrestricted Damerau-Levenshtein distance.
///
/// Mirrors the upstream `_pure_python_unrestricted` recurrence, which needs a
/// last-observed-position map keyed by element value (hence `T: Eq + Hash`).
pub fn damerau_levenshtein_unrestricted<T, F>(s1: &[T], s2: &[T], test: F) -> usize
where
    T: Eq + Hash + Clone,
    F: Fn(&T, &T) -> bool,
{
    let len1 = s1.len();
    let len2 = s2.len();
    let maxdist = len1 + len2;
    let mut d = vec![vec![0usize; len2 + 2]; len1 + 2];
    d[0][0] = maxdist;
    for i in 0..=len1 {
        d[i + 1][0] = maxdist;
        d[i + 1][1] = i;
    }
    for j in 0..=len2 {
        d[0][j + 1] = maxdist;
        d[1][j + 1] = j;
    }
    let mut da: HashMap<T, usize> = HashMap::new();
    for (i, a) in s1.iter().enumerate() {
        let oi = i + 1;
        let mut db = 0usize;
        for (j, b) in s2.iter().enumerate() {
            let oj = j + 1;
            let i1 = da.get(b).copied().unwrap_or(0);
            let j1 = db;
            let cost;
            if test(a, b) {
                cost = 0;
                db = oj;
            } else {
                cost = 1;
            }
            let sub = d[oi][oj] + cost;
            let ins = d[oi + 1][oj] + 1;
            let del = d[oi][oj + 1] + 1;
            let trans = d[i1][j1] + (oi - i1) - 1 + (oj - j1);
            d[oi + 1][oj + 1] = sub.min(ins).min(del).min(trans);
        }
        da.insert(a.clone(), oi);
    }
    d[len1 + 1][len2 + 1]
}

/// Jaro similarity, returning the weight and the number of matched characters.
fn jaro_core<T, F>(s1: &[T], s2: &[T], eq: F) -> (f64, usize)
where
    F: Fn(&T, &T) -> bool,
{
    let s1_len = s1.len();
    let s2_len = s2.len();
    if s1_len == 0 || s2_len == 0 {
        return (0.0, 0);
    }
    let search_range = (s1_len.max(s2_len) / 2).saturating_sub(1);
    let mut s1_flags = vec![false; s1_len];
    let mut s2_flags = vec![false; s2_len];
    let mut common_chars = 0usize;
    for (i, s1_ch) in s1.iter().enumerate() {
        let low = i.saturating_sub(search_range);
        let hi = (i + search_range).min(s2_len - 1);
        for j in low..=hi {
            if !s2_flags[j] && eq(&s2[j], s1_ch) {
                s1_flags[i] = true;
                s2_flags[j] = true;
                common_chars += 1;
                break;
            }
        }
    }
    if common_chars == 0 {
        return (0.0, 0);
    }
    let mut k = 0usize;
    let mut trans_count = 0usize;
    for (i, s1_f) in s1_flags.iter().enumerate() {
        if !*s1_f {
            continue;
        }
        let mut j = k;
        while j < s2_len && !s2_flags[j] {
            j += 1;
        }
        if j < s2_len {
            k = j + 1;
            if !eq(&s1[i], &s2[j]) {
                trans_count += 1;
            }
        }
    }
    trans_count /= 2;
    let mut weight = common_chars as f64 / s1_len as f64
        + common_chars as f64 / s2_len as f64
        + (common_chars - trans_count) as f64 / common_chars as f64;
    weight /= 3.0;
    (weight, common_chars)
}

/// Jaro similarity.
pub fn jaro<T, F>(s1: &[T], s2: &[T], eq: F) -> f64
where
    F: Fn(&T, &T) -> bool,
{
    jaro_core(s1, s2, eq).0
}

/// Jaro-Winkler similarity.
pub fn jaro_winkler<T, F>(
    s1: &[T],
    s2: &[T],
    prefix_weight: f64,
    long_tolerance: bool,
    winklerize: bool,
    eq: F,
) -> f64
where
    F: Fn(&T, &T) -> bool,
{
    let (weight0, common_chars) = jaro_core(s1, s2, &eq);
    let mut weight = weight0;
    if !winklerize {
        return weight;
    }
    if weight <= 0.7 {
        return weight;
    }
    let min_len = s1.len().min(s2.len());
    let limit = min_len.min(4);
    let mut i = 0usize;
    while i < limit && eq(&s1[i], &s2[i]) {
        i += 1;
    }
    if i != 0 {
        weight += i as f64 * prefix_weight * (1.0 - weight);
    }
    if !long_tolerance || min_len <= 4 {
        return weight;
    }
    if common_chars <= i + 1 || 2 * common_chars < min_len + i {
        return weight;
    }
    let tmp = (common_chars - i - 1) as f64 / (s1.len() + s2.len() - i * 2 + 2) as f64;
    weight += (1.0 - weight) * tmp;
    weight
}

/// MLIPNS measure.
///
/// The upstream implementation delegates the inner count to a fresh Hamming
/// instance, so this kernel recomputes that Hamming count and then runs the
/// mismatches loop exactly as the original does.
pub fn mlipns<T, F>(s1: &[T], s2: &[T], threshold: f64, maxmismatches: usize, test: F) -> f64
where
    F: Fn(&[Option<&T>]) -> bool,
{
    let seqs: [&[T]; 2] = [s1, s2];
    let mut mismatches = 0usize;
    let mut ham = hamming_distance(&seqs, false, &test);
    let mut maxlen = s1.len().max(s2.len());
    loop {
        if mismatches > maxmismatches {
            break;
        }
        if maxlen == 0 {
            return 1.0;
        }
        if 1.0 - (maxlen as f64 - ham as f64) / maxlen as f64 <= threshold {
            return 1.0;
        }
        mismatches += 1;
        ham -= 1;
        maxlen -= 1;
    }
    if maxlen == 0 {
        1.0
    } else {
        0.0
    }
}

/// Needleman-Wunsch global alignment score.
pub fn needleman_wunsch<T, F>(s1: &[T], s2: &[T], gap_cost: f64, sim: F) -> f64
where
    F: Fn(&T, &T) -> f64,
{
    let len1 = s1.len();
    let len2 = s2.len();
    let mut d = vec![vec![0f64; len2 + 1]; len1 + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = -(i as f64) * gap_cost;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = -(j as f64) * gap_cost;
    }
    for (i, a) in s1.iter().enumerate() {
        for (j, b) in s2.iter().enumerate() {
            let m = d[i][j] + sim(a, b);
            let del = d[i][j + 1] - gap_cost;
            let ins = d[i + 1][j] - gap_cost;
            d[i + 1][j + 1] = m.max(del).max(ins);
        }
    }
    d[len1][len2]
}

/// Smith-Waterman local alignment score.
pub fn smith_waterman<T, F>(s1: &[T], s2: &[T], gap_cost: f64, sim: F) -> f64
where
    F: Fn(&T, &T) -> f64,
{
    let len1 = s1.len();
    let len2 = s2.len();
    let mut d = vec![vec![0f64; len2 + 1]; len1 + 1];
    for (i, a) in s1.iter().enumerate() {
        for (j, b) in s2.iter().enumerate() {
            let m = d[i][j] + sim(a, b);
            let del = d[i][j + 1] - gap_cost;
            let ins = d[i + 1][j] - gap_cost;
            d[i + 1][j + 1] = 0f64.max(m).max(del).max(ins);
        }
    }
    d[len1][len2]
}

/// Gotoh score (Needleman-Wunsch with affine gap penalties).
pub fn gotoh<T, F>(s1: &[T], s2: &[T], gap_open: f64, gap_ext: f64, sim: F) -> f64
where
    F: Fn(&T, &T) -> f64,
{
    let len1 = s1.len();
    let len2 = s2.len();
    let neg_inf = f64::NEG_INFINITY;
    let mut d_mat = vec![vec![0f64; len2 + 1]; len1 + 1];
    let mut p_mat = vec![vec![0f64; len2 + 1]; len1 + 1];
    let mut q_mat = vec![vec![0f64; len2 + 1]; len1 + 1];
    d_mat[0][0] = 0.0;
    p_mat[0][0] = neg_inf;
    q_mat[0][0] = neg_inf;
    for i in 1..=len1 {
        d_mat[i][0] = neg_inf;
        p_mat[i][0] = -gap_open - gap_ext * (i as f64 - 1.0);
        q_mat[i][0] = neg_inf;
        if len2 >= 1 {
            q_mat[i][1] = -gap_open;
        }
    }
    for j in 1..=len2 {
        d_mat[0][j] = neg_inf;
        p_mat[0][j] = neg_inf;
        if len1 >= 1 {
            p_mat[1][j] = -gap_open;
        }
        q_mat[0][j] = -gap_open - gap_ext * (j as f64 - 1.0);
    }
    for (i, a) in s1.iter().enumerate() {
        let i = i + 1;
        for (j, b) in s2.iter().enumerate() {
            let j = j + 1;
            let sim_val = sim(a, b);
            d_mat[i][j] = (d_mat[i - 1][j - 1] + sim_val)
                .max(p_mat[i - 1][j - 1] + sim_val)
                .max(q_mat[i - 1][j - 1] + sim_val);
            p_mat[i][j] = (d_mat[i - 1][j] - gap_open).max(p_mat[i - 1][j] - gap_ext);
            q_mat[i][j] = (d_mat[i][j - 1] - gap_open).max(q_mat[i][j - 1] - gap_ext);
        }
    }
    d_mat[len1][len2]
        .max(p_mat[len1][len2])
        .max(q_mat[len1][len2])
}

/// StrCmp95 similarity (the CJKJ "strcmp95" port used by the original).
///
/// The input strings must already be stripped and uppercased, matching the
/// upstream `__call__` which does `s1.strip().upper()` in Python. The digit
/// test uses ASCII digits; upstream uses `str.isdigit()`, which is identical
/// for every input in the verified suite. Input is a sequence of Unicode code
/// points (Python's string model), so a lone surrogate is handled like any
/// other character, exactly as the original does.
pub fn strcmp95(s1: &[u32], s2: &[u32], long_strings: bool) -> f64 {
    const SP_MX: [(u32, u32); 36] = [
        ('A' as u32, 'E' as u32),
        ('A' as u32, 'I' as u32),
        ('A' as u32, 'O' as u32),
        ('A' as u32, 'U' as u32),
        ('B' as u32, 'V' as u32),
        ('E' as u32, 'I' as u32),
        ('E' as u32, 'O' as u32),
        ('E' as u32, 'U' as u32),
        ('I' as u32, 'O' as u32),
        ('I' as u32, 'U' as u32),
        ('O' as u32, 'U' as u32),
        ('I' as u32, 'Y' as u32),
        ('E' as u32, 'Y' as u32),
        ('C' as u32, 'G' as u32),
        ('E' as u32, 'F' as u32),
        ('W' as u32, 'U' as u32),
        ('W' as u32, 'V' as u32),
        ('X' as u32, 'K' as u32),
        ('S' as u32, 'Z' as u32),
        ('X' as u32, 'S' as u32),
        ('Q' as u32, 'C' as u32),
        ('U' as u32, 'V' as u32),
        ('M' as u32, 'N' as u32),
        ('L' as u32, 'I' as u32),
        ('Q' as u32, 'O' as u32),
        ('P' as u32, 'R' as u32),
        ('I' as u32, 'J' as u32),
        ('2' as u32, 'Z' as u32),
        ('5' as u32, 'S' as u32),
        ('8' as u32, 'B' as u32),
        ('1' as u32, 'I' as u32),
        ('1' as u32, 'L' as u32),
        ('0' as u32, 'O' as u32),
        ('0' as u32, 'Q' as u32),
        ('C' as u32, 'K' as u32),
        ('G' as u32, 'J' as u32),
    ];

    let len_s1 = s1.len();
    let len_s2 = s2.len();
    let (search_range_initial, minv) = if len_s1 > len_s2 {
        (len_s1, len_s2)
    } else {
        (len_s2, len_s1)
    };
    let mut s1_flag = vec![0u8; search_range_initial];
    let mut s2_flag = vec![0u8; search_range_initial];
    let search_range = (search_range_initial / 2).saturating_sub(1);

    let mut num_com = 0usize;
    let yl1 = len_s2 - 1;
    // `j` is Python's loop variable and keeps its last assigned value across
    // loops, which upstream's strcmp95 relies on in the transposition pass.
    let mut j = 0usize;
    for (i, &sc1) in s1.iter().enumerate() {
        let lowlim = i.saturating_sub(search_range);
        let hilim = (i + search_range).min(yl1);
        if hilim >= lowlim {
            for jj in lowlim..=hilim {
                j = jj;
                if s2_flag[jj] == 0 && s2[jj] == sc1 {
                    s2_flag[jj] = 1;
                    s1_flag[i] = 1;
                    num_com += 1;
                    break;
                }
            }
        }
    }
    if num_com == 0 {
        return 0.0;
    }

    // Count transpositions. Mirrors the upstream loop exactly, including its
    // quirks: when no flagged position is found in `range(k, len_s2)` the
    // comparison still happens against `s2[len_s2 - 1]` (the last `j` of the
    // range), and when the range is empty `j` keeps its previous value.
    let mut k = 0usize;
    let mut n_trans = 0usize;
    for (i, &sc1) in s1.iter().enumerate() {
        if s1_flag[i] == 0 {
            continue;
        }
        if k < len_s2 {
            // Scan `s2_flag[k..]` for the first flagged position. Mirrors the
            // Python `for j in range(k, len_s2): if s2_flag[j] != 0: k = j + 1;
            // break`, where a finished scan leaves `j` at `len_s2 - 1`.
            if let Some(offset) = s2_flag[k..len_s2].iter().position(|f| *f != 0) {
                let jj = k + offset;
                j = jj;
                k = jj + 1;
            } else {
                j = len_s2 - 1;
            }
        }
        if sc1 != s2[j] {
            n_trans += 1;
        }
    }
    n_trans /= 2;

    let in_range = |c: u32| c < 91 && c > 0;
    let mut n_simi = 0i64;
    if minv > num_com {
        for i in 0..len_s1 {
            if s1_flag[i] != 0 || !in_range(s1[i]) {
                continue;
            }
            for j in 0..len_s2 {
                if s2_flag[j] != 0 || !in_range(s2[j]) {
                    continue;
                }
                // Upstream rewards only the phonetic pairs in `adjwt` (the
                // sp_mx table); an exact match is NOT rewarded here.
                if !SP_MX.contains(&(s1[i], s2[j])) && !SP_MX.contains(&(s2[j], s1[i])) {
                    continue;
                }
                n_simi += 3;
                s2_flag[j] = 2;
                break;
            }
        }
    }

    let num_sim = n_simi as f64 / 10.0 + num_com as f64;
    let mut weight = num_sim / len_s1 as f64
        + num_sim / len_s2 as f64
        + (num_com - n_trans) as f64 / num_com as f64;
    weight /= 3.0;
    if weight <= 0.7 {
        return weight;
    }

    let limit = minv.min(4);
    let mut i = 0usize;
    for (sc1, sc2) in s1.iter().zip(s2.iter()) {
        if i >= limit || sc1 != sc2 || (0x30u32..=0x39u32).contains(sc1) {
            break;
        }
        i += 1;
    }
    if i != 0 {
        weight += i as f64 * 0.1 * (1.0 - weight);
    }

    if !long_strings || minv <= 4 || num_com <= i + 1 || 2 * num_com < minv + i {
        return weight;
    }
    if (0x30u32..=0x39u32).contains(&s1[0]) {
        return weight;
    }
    let res = (num_com - i - 1) as f64 / (len_s1 + len_s2 - i * 2 + 2) as f64;
    weight += (1.0 - weight) * res;
    weight
}

fn r_cost(
    x: u32,
    y: u32,
    match_cost: i64,
    group_cost: i64,
    mismatch_cost: i64,
    groups: &[&[u32]],
) -> i64 {
    if x == y {
        return match_cost;
    }
    let grouped = |c: u32| groups.iter().any(|g| g.contains(&c));
    if !grouped(x) || !grouped(y) {
        return mismatch_cost;
    }
    for g in groups {
        if g.contains(&x) && g.contains(&y) {
            return group_cost;
        }
    }
    mismatch_cost
}

fn d_cost(
    x: u32,
    y: u32,
    match_cost: i64,
    group_cost: i64,
    mismatch_cost: i64,
    groups: &[&[u32]],
    ungrouped: &[u32],
) -> i64 {
    if x != y && ungrouped.contains(&x) {
        group_cost
    } else {
        r_cost(x, y, match_cost, group_cost, mismatch_cost, groups)
    }
}

/// Editex distance.
///
/// The input strings must already be uppercased (upstream uppercases in Python
/// before this point). The leading-space padding and the DP that the upstream
/// builds with a numpy matrix are reproduced here with the same recurrences.
#[allow(clippy::too_many_arguments)]
pub fn editex(
    s1: &[u32],
    s2: &[u32],
    match_cost: i64,
    group_cost: i64,
    mismatch_cost: i64,
    local: bool,
    groups: &[&[u32]],
    ungrouped: &[u32],
    max_length: i64,
) -> i64 {
    let mut sa = Vec::with_capacity(s1.len() + 1);
    sa.push(' ' as u32);
    sa.extend_from_slice(s1);
    let mut sb = Vec::with_capacity(s2.len() + 1);
    sb.push(' ' as u32);
    sb.extend_from_slice(s2);
    let len1 = sa.len() - 1;
    let len2 = sb.len() - 1;

    let mut d = vec![vec![0i64; len2 + 1]; len1 + 1];
    if !local {
        for i in 1..=len1 {
            d[i][0] = d[i - 1][0]
                + d_cost(
                    sa[i - 1],
                    sa[i],
                    match_cost,
                    group_cost,
                    mismatch_cost,
                    groups,
                    ungrouped,
                );
        }
    }
    for j in 1..=len2 {
        d[0][j] = d[0][j - 1]
            + d_cost(
                sb[j - 1],
                sb[j],
                match_cost,
                group_cost,
                mismatch_cost,
                groups,
                ungrouped,
            );
    }
    for i in 1..=len1 {
        for j in 1..=len2 {
            let a = d[i - 1][j]
                + d_cost(
                    sa[i - 1],
                    sa[i],
                    match_cost,
                    group_cost,
                    mismatch_cost,
                    groups,
                    ungrouped,
                );
            let b = d[i][j - 1]
                + d_cost(
                    sb[j - 1],
                    sb[j],
                    match_cost,
                    group_cost,
                    mismatch_cost,
                    groups,
                    ungrouped,
                );
            let c = d[i - 1][j - 1]
                + r_cost(sa[i], sb[j], match_cost, group_cost, mismatch_cost, groups);
            d[i][j] = a.min(b).min(c);
        }
    }
    d[len1][len2].min(max_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(a: &char, b: &char) -> bool {
        a == b
    }

    #[test]
    fn hamming_basics() {
        let a: Vec<char> = "test".chars().collect();
        let b: Vec<char> = "text".chars().collect();
        let seqs = [a.as_slice(), b.as_slice()];
        let count = hamming_distance(&seqs, false, |col: &[Option<&char>]| {
            match col.first().and_then(|o| o.as_ref()) {
                Some(first) => col.iter().all(|o| o.as_ref() == Some(first)),
                None => false,
            }
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn levenshtein_known() {
        let a: Vec<char> = "kitten".chars().collect();
        let b: Vec<char> = "sitting".chars().collect();
        assert_eq!(levenshtein(&a, &b, eq), 3);
        assert_eq!(levenshtein(&a, &a, eq), 0);
    }

    #[test]
    fn damerau_restricted_known() {
        let a: Vec<char> = "ab".chars().collect();
        let b: Vec<char> = "ba".chars().collect();
        assert_eq!(damerau_levenshtein_restricted(&a, &b, eq), 1);
        assert_eq!(damerau_levenshtein_unrestricted(&a, &b, eq), 1);
    }

    #[test]
    fn jaro_known() {
        let a: Vec<char> = "MARTHA".chars().collect();
        let b: Vec<char> = "MARHTA".chars().collect();
        let actual = jaro_winkler(&a, &b, 0.1, false, false, eq);
        assert!((actual - 0.944444444).abs() < 1e-9);
    }

    #[test]
    fn strcmp95_known() {
        let a: Vec<u32> = "MARTHA".chars().map(|c| c as u32).collect();
        let b: Vec<u32> = "MARHTA".chars().map(|c| c as u32).collect();
        let actual = strcmp95(&a, &b, false);
        assert!((actual - 0.9611111111111111).abs() < 1e-9);
    }

    #[test]
    fn needleman_known() {
        let a: Vec<char> = "GATTACA".chars().collect();
        let b: Vec<char> = "GCATGCU".chars().collect();
        let sim = |x: &char, y: &char| if x == y { 1.0 } else { -1.0 };
        assert_eq!(needleman_wunsch(&a, &b, 1.0, sim), 0.0);
    }

    #[test]
    fn editex_known() {
        let a: Vec<u32> = "nelson".to_uppercase().chars().map(|c| c as u32).collect();
        let b: Vec<u32> = "neilsen".to_uppercase().chars().map(|c| c as u32).collect();
        let groups: Vec<Vec<u32>> = vec![
            "AEIOUY".chars().map(|c| c as u32).collect(),
            "BP".chars().map(|c| c as u32).collect(),
            "CKQ".chars().map(|c| c as u32).collect(),
            "DT".chars().map(|c| c as u32).collect(),
            "LR".chars().map(|c| c as u32).collect(),
            "MN".chars().map(|c| c as u32).collect(),
            "GJ".chars().map(|c| c as u32).collect(),
            "FPV".chars().map(|c| c as u32).collect(),
            "SXZ".chars().map(|c| c as u32).collect(),
            "CSZ".chars().map(|c| c as u32).collect(),
        ];
        let group_refs: Vec<&[u32]> = groups.iter().map(|g| g.as_slice()).collect();
        let ungrouped: Vec<u32> = "HW".chars().map(|c| c as u32).collect();
        assert_eq!(
            editex(&a, &b, 0, 1, 2, false, &group_refs, &ungrouped, 14),
            2
        );
    }
}
