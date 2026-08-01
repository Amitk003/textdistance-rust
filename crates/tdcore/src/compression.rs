//! Compression-based distance kernels (the NCD family).
//!
//! Mirrors `algorithms/compression_based.py` upstream. The pure-Rust
//! compressors (arithmetic coding, run-length, Burrows-Wheeler, square-root
//! and entropy "sizes") live here. The binary compressors (bz2, zlib, lzma)
//! delegate to the same C libraries CPython uses, wrapped by the `codec`
//! crate in the extension layer.

use num_bigint::{BigInt, BigUint};
use num_integer::Integer;
use std::cmp::Ordering;
use std::collections::HashMap;

/// An exact non-negative rational, reduced like Python's `fractions.Fraction`.
#[derive(Clone, Debug)]
struct Frac {
    num: BigUint,
    den: BigUint,
}

impl Frac {
    fn new(num: BigUint, den: BigUint) -> Frac {
        let g = num.gcd(&den);
        Frac {
            num: num / &g,
            den: den / &g,
        }
    }

    fn zero() -> Frac {
        Frac::new(BigUint::from(0u8), BigUint::from(1u8))
    }

    fn one() -> Frac {
        Frac::new(BigUint::from(1u8), BigUint::from(1u8))
    }

    fn add(&self, other: &Frac) -> Frac {
        Frac::new(
            &self.num * &other.den + &other.num * &self.den,
            &self.den * &other.den,
        )
    }

    fn mul(&self, other: &Frac) -> Frac {
        Frac::new(&self.num * &other.num, &self.den * &other.den)
    }
}

impl PartialEq for Frac {
    fn eq(&self, other: &Self) -> bool {
        &self.num * &other.den == &other.num * &self.den
    }
}

impl PartialOrd for Frac {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some((&self.num * &other.den).cmp(&(&other.num * &self.den)))
    }
}

/// Order characters by their count, descending, ties by first occurrence,
/// matching Python's `collections.Counter.most_common()` (a stable sort).
fn most_common(counts: &HashMap<u32, u64>, order: &[u32]) -> Vec<(u32, u64)> {
    let mut items: Vec<(u32, u64)> = order.iter().map(|key| (*key, counts[key])).collect();
    items.sort_by_key(|b| std::cmp::Reverse(b.1));
    items
}

/// Arithmetic coding of `data` over its own per-character probabilities,
/// returning the exact reduced fraction, mirroring `ArithNCD._compress`.
///
/// `data` is a sequence of Unicode code points (Python's string model, which
/// also holds lone surrogates). Returns `(numerator, denominator)` of the
/// encoded fraction.
pub fn arith_compress(data: &[u32], terminator: Option<u32>) -> (BigInt, BigInt) {
    let mut counts: HashMap<u32, u64> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for &ch in data {
        if !counts.contains_key(&ch) {
            order.push(ch);
        }
        *counts.entry(ch).or_insert(0) += 1;
    }
    if let Some(t) = terminator {
        if !counts.contains_key(&t) {
            order.push(t);
        }
        counts.insert(t, 1);
    }

    let total = BigUint::from(counts.values().sum::<u64>());
    let mut cum = BigUint::from(0u8);
    let mut probs: Vec<(u32, Frac, Frac)> = Vec::new();
    for (key, count) in most_common(&counts, &order) {
        let count = BigUint::from(count);
        probs.push((
            key,
            Frac::new(cum.clone(), total.clone()),
            Frac::new(count.clone(), total.clone()),
        ));
        cum += count;
    }

    let mut data2: Vec<u32>;
    let data_ref: &[u32] = match terminator {
        Some(t) => {
            data2 = if data.contains(&t) {
                data.iter().copied().filter(|&ch| ch != t).collect()
            } else {
                data.to_vec()
            };
            data2.push(t);
            &data2
        }
        None => data,
    };

    let mut start = Frac::zero();
    let mut width = Frac::one();
    let prob_map: HashMap<u32, (&Frac, &Frac)> =
        probs.iter().map(|(k, s, w)| (*k, (s, w))).collect();
    for &ch in data_ref {
        let (prob_start, prob_width) = prob_map[&ch];
        start = start.add(&prob_start.mul(&width));
        width = width.mul(prob_width);
    }

    let end = start.add(&width);
    // Mirrors `ArithNCD._compress`: the candidate fraction is built from the
    // current denominator, and only then does the denominator double. The two
    // are separate state, so the fraction checked in each iteration always
    // pairs a numerator with the denominator it was computed from.
    let mut cand_num = BigUint::from(0u8);
    let mut cand_den = BigUint::from(1u8);
    let mut build_den = BigUint::from(1u8);
    loop {
        let candidate = Frac::new(cand_num.clone(), cand_den.clone());
        if start <= candidate && candidate < end {
            return (BigInt::from(candidate.num), BigInt::from(candidate.den));
        }
        cand_num = (&start.num * &build_den) / &start.den + 1u8;
        cand_den = build_den.clone();
        build_den *= 2u8;
    }
}

/// Run-length encoding over characters, mirroring `RLENCD._compress`.
/// Operates on code points and returns the encoded code points (the count
/// digits are ASCII code points).
pub fn rle(data: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let mut iter = data.iter().copied();
    let mut current = iter.next();
    while let Some(ch) = current {
        let mut n: u64 = 1;
        loop {
            match iter.next() {
                Some(next) if next == ch => n += 1,
                Some(next) => {
                    current = Some(next);
                    break;
                }
                None => {
                    current = None;
                    break;
                }
            }
        }
        if n > 2 {
            out.extend(n.to_string().bytes().map(|b| b as u32));
            out.push(ch);
        } else if n == 1 {
            out.push(ch);
        } else {
            out.push(ch);
            out.push(ch);
        }
    }
    out
}

/// Burrows-Wheeler transform over characters, mirroring `BWTRLENCD._compress`.
///
/// Appends `terminator` when absent, sorts every rotation by code point (the
/// same order as Python string comparison), and returns the last character of
/// each rotation as a code point sequence.
pub fn bwt(data: &[u32], terminator: u32) -> Vec<u32> {
    if data.is_empty() {
        return vec![terminator];
    }
    if data.contains(&terminator) {
        return data.to_vec();
    }
    let mut data2: Vec<u32> = data.to_vec();
    data2.push(terminator);
    let n = data2.len();
    let mut rotations: Vec<Vec<u32>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut rotation = Vec::with_capacity(n);
        rotation.extend_from_slice(&data2[i..]);
        rotation.extend_from_slice(&data2[..i]);
        rotations.push(rotation);
    }
    rotations.sort();
    rotations.iter().map(|rotation| rotation[n - 1]).collect()
}

/// Sum of square roots of the per-element counts, mirroring
/// `SqrtNCD._get_size`. Iteration order is first occurrence, matching the
/// insertion order of a Python `Counter` (which drives `sum()` upstream).
pub fn sqrt_size(data: &[u32]) -> f64 {
    let mut counts: HashMap<u32, u64> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for &ch in data {
        if !counts.contains_key(&ch) {
            order.push(ch);
        }
        *counts.entry(ch).or_insert(0) += 1;
    }
    let mut total = 0.0f64;
    for ch in order {
        total += (counts[&ch] as f64).sqrt();
    }
    total
}

/// Shannon entropy of the per-element counts, mirroring `EntropyNCD._compress`.
///
/// `base` is the log base. Accumulation follows Python's operation order
/// (`p * (ln(p) / ln(base))`, summed over first-occurrence order) so the
/// result is bit-identical to the reference.
pub fn entropy(data: &[u32], base: f64) -> f64 {
    let total_count = data.len();
    if total_count == 0 {
        return 0.0;
    }
    let mut counts: HashMap<u32, u64> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for &ch in data {
        if !counts.contains_key(&ch) {
            order.push(ch);
        }
        *counts.entry(ch).or_insert(0) += 1;
    }
    let base = base.ln();
    let mut entropy = 0.0f64;
    for ch in order {
        let p = counts[&ch] as f64 / total_count as f64;
        entropy -= p * (p.ln() / base);
    }
    entropy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(s: &str) -> Vec<u32> {
        s.chars().map(|c| c as u32).collect()
    }

    fn frac(s: &str) -> (BigInt, BigInt) {
        arith_compress(&cp(s), None)
    }

    #[test]
    fn arith_single_symbol_does_not_narrow() {
        assert_eq!(frac("x"), (BigInt::from(0u8), BigInt::from(1u8)));
        assert_eq!(frac(""), (BigInt::from(0u8), BigInt::from(1u8)));
    }

    #[test]
    fn arith_banana_terminator_matches_reference() {
        let (n, d) = arith_compress(&cp("BANANA"), Some('\u{0}' as u32));
        assert_eq!(n, BigInt::from(1525u32));
        assert_eq!(d, BigInt::from(2048u32));
    }

    #[test]
    fn arith_banana_plain_matches_reference() {
        let (n, d) = arith_compress(&cp("BANANA"), None);
        assert_eq!(n, BigInt::from(113u32));
        assert_eq!(d, BigInt::from(128u32));
    }

    #[test]
    fn arith_terminator_removed_then_readded() {
        let t = Some('\u{0}' as u32);
        let (n, d) = arith_compress(&cp("A\u{0}A"), t);
        // data becomes "A\u{0}" after removal and re-append.
        let (n2, d2) = arith_compress(&cp("AA"), t);
        assert_eq!((n, d), (n2, d2));
    }

    #[test]
    fn rle_runs() {
        assert_eq!(rle(&cp("aaabbbbc")), cp("3a4bc"));
        assert_eq!(rle(&cp("abc")), cp("abc"));
        assert_eq!(rle(&cp("aabbcc")), cp("aabbcc"));
        assert_eq!(rle(&[]), vec![]);
        assert_eq!(rle(&cp("aaaaaaaaaa")), cp("10a"));
    }

    #[test]
    fn bwt_transforms() {
        let t = '\u{0}' as u32;
        assert_eq!(bwt(&cp("test"), t), cp("ttes\u{0}"));
        assert_eq!(bwt(&cp("banana"), t), cp("annb\u{0}aa"));
        assert_eq!(bwt(&[], t), vec![t]);
        assert_eq!(bwt(&cp("a\u{0}b"), t), cp("a\u{0}b"));
    }

    #[test]
    fn sqrt_size_matches_reference() {
        assert_eq!(sqrt_size(&cp("test")), 3.414213562373095);
        assert_eq!(sqrt_size(&[]), 0.0);
        assert_eq!(sqrt_size(&cp("aaaa")), 2.0);
    }

    #[test]
    fn entropy_matches_reference() {
        assert_eq!(entropy(&cp("test"), 2.0), 1.5);
        assert_eq!(entropy(&cp("aaa"), 2.0), 0.0);
        assert_eq!(entropy(&[], 2.0), 0.0);
    }

    #[test]
    fn entropy_base_changes_scale() {
        // entropy with base b equals (-sum p ln p) / ln(b).
        let log2 = entropy(&cp("test"), 2.0);
        let log10 = entropy(&cp("test"), 10.0);
        assert!((log10 * 10.0_f64.ln() - log2 * 2.0_f64.ln()).abs() < 1e-12);
    }
}
