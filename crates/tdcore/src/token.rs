//! Token-family kernels.
//!
//! The set/counter arithmetic shared by Jaccard, Sorensen, Tversky, Overlap,
//! Cosine, and Bag. The original implements these with Python `Counter`
//! intersection/union/difference; here the same arithmetic runs over generic
//! key -> count maps so the FFI layer can hash arbitrary Python objects with
//! Python's own hash and equality. The per-algorithm ratio formulas stay in the
//! Python adapter, exactly as written upstream.
//!
//! Nothing in this module touches the Python runtime.

use std::collections::HashMap;
use std::hash::Hash;

/// Aggregate statistics over N counters.
#[derive(Debug)]
pub struct TokenStats {
    /// Cardinality of the multiset intersection (min counts), or the number of
    /// shared keys when `as_set` is set.
    pub intersection: f64,
    /// Cardinality of the multiset union (max counts), or the number of
    /// distinct keys when `as_set` is set.
    pub union: f64,
    /// Total element count per sequence (unique count when `as_set`).
    pub counts: Vec<f64>,
}

/// Counter statistics over arbitrary key -> count maps.
pub fn token_stats<K>(counters: &[HashMap<K, f64>], as_set: bool) -> TokenStats
where
    K: Eq + Hash + Clone,
{
    let mut union: HashMap<K, f64> = HashMap::new();
    for c in counters {
        for (k, v) in c {
            let e = union.entry(k.clone()).or_insert(0.0);
            if *v > *e {
                *e = *v;
            }
        }
    }
    let mut inter: HashMap<K, f64> = match counters.first() {
        Some(c) => c.clone(),
        None => HashMap::new(),
    };
    for c in &counters[1..] {
        let mut next: HashMap<K, f64> = HashMap::new();
        for (k, v) in inter {
            if let Some(&cv) = c.get(&k) {
                next.insert(k, v.min(cv));
            }
        }
        inter = next;
    }
    let cardinality = |m: &HashMap<K, f64>| -> f64 {
        if as_set {
            m.len() as f64
        } else {
            // explicit fold so an empty map sums to +0.0 (iterator sum() is -0.0)
            m.values().fold(0.0, |acc, v| acc + v)
        }
    };
    let mut counts = Vec::with_capacity(counters.len());
    for c in counters {
        counts.push(cardinality(c));
    }
    TokenStats {
        intersection: cardinality(&inter),
        union: cardinality(&union),
        counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&'static str, f64)]) -> HashMap<&'static str, f64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn jaccard_stats() {
        let a = map(&[("t", 2.0), ("e", 1.0), ("s", 1.0)]);
        let b = map(&[("t", 1.0), ("e", 1.0), ("x", 1.0)]);
        let stats = token_stats(&[a, b], false);
        assert_eq!(stats.intersection, 2.0);
        assert_eq!(stats.union, 5.0);
        assert_eq!(stats.counts, vec![4.0, 3.0]);
    }

    #[test]
    fn as_set_stats() {
        let a = map(&[("t", 2.0), ("e", 1.0)]);
        let b = map(&[("t", 1.0), ("x", 3.0)]);
        let stats = token_stats(&[a, b], true);
        assert_eq!(stats.intersection, 1.0);
        assert_eq!(stats.union, 3.0);
        assert_eq!(stats.counts, vec![2.0, 2.0]);
    }

    #[test]
    fn zero_intersection_is_positive_zero() {
        let a = map(&[("n", 2.0), ("e", 1.0), ("l", 1.0), ("s", 1.0), ("o", 1.0)]);
        let b = map(&[("M", 1.0), ("A", 2.0), ("R", 1.0), ("T", 1.0), ("H", 1.0)]);
        let stats = token_stats(&[a, b], false);
        assert!(!stats.intersection.is_sign_negative());
        assert_eq!(stats.intersection.to_bits(), 0.0f64.to_bits());
    }
}
