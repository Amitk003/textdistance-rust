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
fn most_common(counts: &HashMap<String, u64>, order: &[String]) -> Vec<(String, u64)> {
    let mut items: Vec<(String, u64)> =
        order.iter().map(|key| (key.clone(), counts[key])).collect();
    items.sort_by_key(|b| std::cmp::Reverse(b.1));
    items
}

/// Arithmetic coding of `data` over its own per-character probabilities,
/// returning the exact reduced fraction, mirroring `ArithNCD._compress`.
///
/// Returns `(numerator, denominator)` of the encoded fraction.
pub fn arith_compress(data: &str, terminator: Option<&str>) -> (BigInt, BigInt) {
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for ch in data.chars() {
        let key = ch.to_string();
        if !counts.contains_key(&key) {
            order.push(key.clone());
        }
        *counts.entry(key).or_insert(0) += 1;
    }
    if let Some(t) = terminator {
        if !counts.contains_key(t) {
            order.push(t.to_string());
        }
        counts.insert(t.to_string(), 1);
    }

    let total = BigUint::from(counts.values().sum::<u64>());
    let mut cum = BigUint::from(0u8);
    let mut probs: Vec<(String, Frac, Frac)> = Vec::new();
    for (key, count) in most_common(&counts, &order) {
        let count = BigUint::from(count);
        probs.push((
            key,
            Frac::new(cum.clone(), total.clone()),
            Frac::new(count.clone(), total.clone()),
        ));
        cum += count;
    }

    let data2: String;
    let data_ref: &str = match terminator {
        Some(t) => {
            data2 = if data.contains(t) {
                data.replace(t, "")
            } else {
                data.to_string()
            } + t;
            &data2
        }
        None => data,
    };

    let mut start = Frac::zero();
    let mut width = Frac::one();
    let prob_map: HashMap<&str, (&Frac, &Frac)> =
        probs.iter().map(|(k, s, w)| (k.as_str(), (s, w))).collect();
    for ch in data_ref.chars() {
        let (prob_start, prob_width) = prob_map[ch.to_string().as_str()];
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
pub fn rle(data: &str) -> String {
    let mut out = String::new();
    let mut chars = data.chars();
    let mut current = chars.next();
    while let Some(ch) = current {
        let mut n: u64 = 1;
        loop {
            match chars.next() {
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
            out.push_str(&n.to_string());
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
/// Appends `terminator` when absent, sorts every rotation, and returns the
/// last character of each rotation.
pub fn bwt(data: &str, terminator: &str) -> String {
    if data.is_empty() {
        return terminator.to_string();
    }
    if data.contains(terminator) {
        return data.to_string();
    }
    let data2: String = format!("{data}{terminator}");
    let chars: Vec<char> = data2.chars().collect();
    let n = chars.len();
    let mut rotations: Vec<Vec<char>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut rotation = Vec::with_capacity(n);
        rotation.extend_from_slice(&chars[i..]);
        rotation.extend_from_slice(&chars[..i]);
        rotations.push(rotation);
    }
    rotations.sort();
    rotations.iter().map(|rotation| rotation[n - 1]).collect()
}

/// Sum of square roots of the per-element counts, mirroring
/// `SqrtNCD._get_size`. Iteration order is first occurrence, matching the
/// insertion order of a Python `Counter` (which drives `sum()` upstream).
pub fn sqrt_size(data: &str) -> f64 {
    let mut counts: HashMap<char, u64> = HashMap::new();
    let mut order: Vec<char> = Vec::new();
    for ch in data.chars() {
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
pub fn entropy(data: &str, base: f64) -> f64 {
    let total_count = data.chars().count();
    if total_count == 0 {
        return 0.0;
    }
    let mut counts: HashMap<char, u64> = HashMap::new();
    let mut order: Vec<char> = Vec::new();
    for ch in data.chars() {
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

    fn frac(s: &str) -> (BigInt, BigInt) {
        arith_compress(s, None)
    }

    #[test]
    fn arith_single_symbol_does_not_narrow() {
        assert_eq!(frac("x"), (BigInt::from(0u8), BigInt::from(1u8)));
        assert_eq!(frac(""), (BigInt::from(0u8), BigInt::from(1u8)));
    }

    #[test]
    fn arith_banana_terminator_matches_reference() {
        let (n, d) = arith_compress("BANANA", Some("\u{0}"));
        assert_eq!(n, BigInt::from(1525u32));
        assert_eq!(d, BigInt::from(2048u32));
    }

    #[test]
    fn arith_banana_plain_matches_reference() {
        let (n, d) = arith_compress("BANANA", None);
        assert_eq!(n, BigInt::from(113u32));
        assert_eq!(d, BigInt::from(128u32));
    }

    #[test]
    fn arith_terminator_removed_then_readded() {
        let (n, d) = arith_compress("A\u{0}A", Some("\u{0}"));
        // data becomes "A\u{0}" after removal and re-append.
        let (n2, d2) = arith_compress("AA", Some("\u{0}"));
        assert_eq!((n, d), (n2, d2));
    }

    #[test]
    fn rle_runs() {
        assert_eq!(rle("aaabbbbc"), "3a4bc");
        assert_eq!(rle("abc"), "abc");
        assert_eq!(rle("aabbcc"), "aabbcc");
        assert_eq!(rle(""), "");
        assert_eq!(rle("aaaaaaaaaa"), "10a");
    }

    #[test]
    fn bwt_transforms() {
        assert_eq!(bwt("test", "\u{0}"), "ttes\u{0}");
        assert_eq!(bwt("banana", "\u{0}"), "annb\u{0}aa");
        assert_eq!(bwt("", "\u{0}"), "\u{0}");
        assert_eq!(bwt("a\u{0}b", "\u{0}"), "a\u{0}b");
    }

    #[test]
    fn sqrt_size_matches_reference() {
        assert_eq!(sqrt_size("test"), 3.414213562373095);
        assert_eq!(sqrt_size(""), 0.0);
        assert_eq!(sqrt_size("aaaa"), 2.0);
    }

    #[test]
    fn entropy_matches_reference() {
        assert_eq!(entropy("test", 2.0), 1.5);
        assert_eq!(entropy("aaa", 2.0), 0.0);
        assert_eq!(entropy("", 2.0), 0.0);
    }

    #[test]
    fn entropy_base_changes_scale() {
        // entropy with base b equals (-sum p ln p) / ln(b).
        let log2 = entropy("test", 2.0);
        let log10 = entropy("test", 10.0);
        assert!((log10 * 10.0_f64.ln() - log2 * 2.0_f64.ln()).abs() < 1e-12);
    }
}
