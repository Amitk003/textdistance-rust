//! Sequence-family kernels.
//!
//! LCSSeq, LCSStr, and Ratcliff-Obershelp. LCSSeq replicates the upstream
//! dynamic-programming matrix walk (including its tie-breaking) and returns the
//! matched indices into the first sequence. LCSStr's standard path replicates
//! `difflib.SequenceMatcher.find_longest_match` for the short-string case the
//! original routes through difflib (autojunk and isjunk are both inactive for
//! that path). Ratcliff-Obershelp is the recursive common-substring split.
//!
//! Every kernel is generic over the element type and takes a comparison
//! predicate, exactly like the edit kernels. Nothing in this module touches the
//! Python runtime.

use std::collections::HashMap;
use std::hash::Hash;

/// Longest common subsequence: matched indices into `s1`, in increasing order.
///
/// The DP table and the backward walk mirror `LCSSeq._dynamic`, so the
/// tie-breaking matches the original exactly and the adapter can rebuild the
/// result with the same `element + result` expression.
pub fn lcsseq<T, F>(s1: &[T], s2: &[T], test: F) -> Vec<usize>
where
    F: Fn(&T, &T) -> bool,
{
    let n = s1.len();
    let m = s2.len();
    let mut lengths = vec![vec![0usize; m + 1]; n + 1];
    for (i, a) in s1.iter().enumerate() {
        for (j, b) in s2.iter().enumerate() {
            if test(a, b) {
                lengths[i + 1][j + 1] = lengths[i][j] + 1;
            } else {
                lengths[i + 1][j + 1] = lengths[i + 1][j].max(lengths[i][j + 1]);
            }
        }
    }
    let mut indices = Vec::new();
    let (mut i, mut j) = (n, m);
    while i != 0 && j != 0 {
        if lengths[i][j] == lengths[i - 1][j] {
            i -= 1;
        } else if lengths[i][j] == lengths[i][j - 1] {
            j -= 1;
        } else {
            indices.push(i - 1);
            i -= 1;
            j -= 1;
        }
    }
    indices.reverse();
    indices
}

/// Longest common substring via the difflib `find_longest_match` recurrence.
///
/// Returns `(besti, bestsize)`: the start index into `s1` and the match length.
/// This replicates the pure algorithm with no junk configured, which is exactly
/// the state difflib is in for the strings the original sends here (both under
/// 200 elements, so autojunk never engages).
pub fn lcsstr_standard<T, F>(s1: &[T], s2: &[T], test: F) -> (usize, usize)
where
    T: Eq + Hash,
    F: Fn(&T, &T) -> bool,
{
    let mut b2j: HashMap<&T, Vec<usize>> = HashMap::new();
    for (j, b) in s2.iter().enumerate() {
        b2j.entry(b).or_default().push(j);
    }
    let mut j2len: HashMap<usize, usize> = HashMap::new();
    let mut besti = 0usize;
    let mut bestj = 0usize;
    let mut bestsize = 0usize;
    for (i, a) in s1.iter().enumerate() {
        let mut newj2len: HashMap<usize, usize> = HashMap::new();
        let entries = b2j.get(a).cloned().unwrap_or_default();
        for j in entries {
            let prev = j
                .checked_sub(1)
                .and_then(|p| j2len.get(&p))
                .copied()
                .unwrap_or(0);
            let k = prev + 1;
            newj2len.insert(j, k);
            if k > bestsize {
                besti = i + 1 - k;
                bestj = j + 1 - k;
                bestsize = k;
            }
        }
        j2len = newj2len;
    }
    while besti > 0 && bestj > 0 && test(&s1[besti - 1], &s2[bestj - 1]) {
        besti -= 1;
        bestj -= 1;
        bestsize += 1;
    }
    while besti + bestsize < s1.len()
        && bestj + bestsize < s2.len()
        && test(&s1[besti + bestsize], &s2[bestj + bestsize])
    {
        bestsize += 1;
    }
    (besti, bestsize)
}

/// First occurrence of `needle` in `haystack`, or `usize::MAX`.
fn find_subseq<T>(haystack: &[T], needle: &[T]) -> usize
where
    T: PartialEq,
{
    if needle.is_empty() {
        return 0;
    }
    if needle.len() > haystack.len() {
        return usize::MAX;
    }
    for start in 0..=haystack.len() - needle.len() {
        if &haystack[start..start + needle.len()] == needle {
            return start;
        }
    }
    usize::MAX
}

/// Longest common substring content over N sequences.
///
/// Mirrors `LCSStr.__call__`: two sequences under 200 elements use the
/// difflib-standard path, everything else uses the sliding-window scan of the
/// shortest sequence.
fn lcsstr_content<T>(seqs: &[Vec<T>]) -> Vec<T>
where
    T: Eq + Hash + Clone,
{
    if seqs.len() == 2 && seqs.iter().map(|s| s.len()).max().unwrap_or(0) < 200 {
        let (besti, bestsize) = lcsstr_standard(&seqs[0], &seqs[1], |a: &T, b: &T| a == b);
        return seqs[0][besti..besti + bestsize].to_vec();
    }
    let Some(short) = seqs.iter().min_by_key(|s| s.len()) else {
        return Vec::new();
    };
    for n in (1..=short.len()).rev() {
        for start in 0..=short.len() - n {
            let window = &short[start..start + n];
            if seqs.iter().all(|s| find_subseq(s, window) != usize::MAX) {
                return window.to_vec();
            }
        }
    }
    Vec::new()
}

/// Sum of the lengths of the matched blocks in the Ratcliff-Obershelp
/// recursion, mirroring `RatcliffObershelp._find`.
pub fn ratcliff_obershelp<T>(seqs: &[Vec<T>]) -> usize
where
    T: Eq + Hash + Clone,
{
    let subseq = lcsstr_content(seqs);
    let length = subseq.len();
    if length == 0 {
        return 0;
    }
    let mut total = length;
    let before: Vec<Vec<T>> = seqs
        .iter()
        .map(|s| {
            let pos = find_subseq(s, &subseq);
            s[..pos.min(s.len())].to_vec()
        })
        .collect();
    let after: Vec<Vec<T>> = seqs
        .iter()
        .map(|s| {
            let pos = find_subseq(s, &subseq).min(s.len());
            s[pos + length..].to_vec()
        })
        .collect();
    total += ratcliff_obershelp(&before);
    total += ratcliff_obershelp(&after);
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(a: &char, b: &char) -> bool {
        a == b
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn lcsseq_str(a: &str, b: &str) -> String {
        let a = chars(a);
        let b = chars(b);
        let indices = lcsseq(&a, &b, eq);
        indices.into_iter().map(|i| a[i]).collect()
    }

    #[test]
    fn lcsseq_known() {
        assert_eq!(lcsseq_str("ab", "cd"), "");
        assert_eq!(lcsseq_str("abcd", "abcd"), "abcd");
        assert_eq!(lcsseq_str("test", "text"), "tet");
        assert_eq!(lcsseq_str("DIXON", "DICKSONX"), "DION");
    }

    #[test]
    fn lcsseq_long() {
        let a = "a".repeat(80);
        let b = "b".repeat(80);
        assert_eq!(lcsseq_str(&a, &b), "");
        assert_eq!(lcsseq_str(&a, &a), a);
    }

    #[test]
    fn lcsstr_standard_known() {
        let cases: [(&str, &str, &str); 7] = [
            ("ab", "abcd", "ab"),
            ("abcd", "ab", "ab"),
            ("abcd", "bc", "bc"),
            ("bc", "abcd", "bc"),
            ("abcd", "cd", "cd"),
            ("abcd", "ef", ""),
            ("ef", "abcd", ""),
        ];
        for (a, b, expected) in cases {
            let a = chars(a);
            let b = chars(b);
            let (besti, bestsize) = lcsstr_standard(&a, &b, eq);
            let actual: String = a[besti..besti + bestsize].iter().collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn ratcliff_obershelp_known() {
        let a = chars("abraham");
        let b = chars("bram");
        assert_eq!(ratcliff_obershelp(&[a, b]), 4);
        let a = chars("spam");
        let b = chars("qwer");
        assert_eq!(ratcliff_obershelp(&[a, b]), 0);
        let a = chars("MARTHA");
        let b = chars("MARHTA");
        assert_eq!(ratcliff_obershelp(&[a, b]), 5);
    }

    #[test]
    fn ratcliff_obershelp_three() {
        let a = chars("test");
        let b = chars("text");
        let c = chars("tempest");
        assert_eq!(ratcliff_obershelp(&[a, b, c]), 3);
    }
}
