//! `tdc` is a small command line interface over the pure-Rust kernels in
//! `tdcore`. It computes the same four metrics as the Python API
//! (distance, similarity, normalized_distance, normalized_similarity) for a
//! curated set of string algorithms, without any Python involved.
//!
//! Usage:
//!   tdc list
//!   tdc distance <algorithm> <s1> <s2>
//!   tdc similarity <algorithm> <s1> <s2>
//!   tdc normalized_distance <algorithm> <s1> <s2>
//!   tdc normalized_similarity <algorithm> <s1> <s2>
//!   tdc --version

use std::process::exit;
use tdcore::edit;
use tdcore::simple::length_distance;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// Column predicate used by the multi-sequence kernels: every present
/// character must equal the first one. Mirrors the char fast path in the
/// Python extension.
fn char_col_eq(col: &[Option<&char>]) -> bool {
    match col.first().and_then(|o| o.as_ref()) {
        Some(first) => col.iter().all(|o| match o {
            Some(c) => c == first,
            None => false,
        }),
        None => false,
    }
}

/// Upstream StrCmp95 strips whitespace and uppercases its input in the
/// adapter before the kernel runs. Preprocess exactly that way.
fn strcmp95_prepare(s: &[char]) -> Vec<char> {
    let mut start = 0;
    let mut end = s.len();
    while start < end && s[start].is_whitespace() {
        start += 1;
    }
    while end > start && s[end - 1].is_whitespace() {
        end -= 1;
    }
    s[start..end]
        .iter()
        .flat_map(|c| c.to_uppercase())
        .collect()
}

enum Metric {
    Distance,
    Similarity,
    NormalizedDistance,
    NormalizedSimilarity,
}

/// The four public metrics derive from a core score plus the per-class
/// maximum/minimum and the normalized formulas, exactly as in the Python
/// adapter.
///
/// The alignment classes are special: their `__call__` returns the DP score
/// (a similarity), `distance` is `-similarity` (NW/Gotoh) or
/// `maximum - similarity` (Smith-Waterman), and Needleman-Wunsch/Gotoh use
/// rescaled normalized formulas with a negative minimum. See
/// reference/textdistance/textdistance/algorithms/edit_based.py.
enum Family {
    /// Base: core is a distance, maximum is the longer length.
    Base,
    /// BaseSimilarity with maximum 1: core is a 0..1 similarity.
    SimMax1,
    /// BaseSimilarity with maximum = min length (Smith-Waterman).
    SmithWaterman,
    /// Core is the DP score; maximum = max length, minimum = -max length.
    NeedlemanWunsch,
    /// Core is the DP score; maximum = min length, minimum = -min length.
    Gotoh,
}

struct Algo {
    name: &'static str,
    family: Family,
    core: fn(&[char], &[char]) -> f64,
    /// Input preprocessing before the empty-input quick answer and the
    /// kernel (identity for most algorithms).
    prepare: fn(&[char]) -> Vec<char>,
}

fn identity_prepare(s: &[char]) -> Vec<char> {
    s.to_vec()
}

const ALGORITHMS: &[Algo] = &[
    Algo {
        name: "levenshtein",
        family: Family::Base,
        core: |a, b| edit::levenshtein(a, b, |x, y| x == y) as f64,
        prepare: identity_prepare,
    },
    Algo {
        name: "damerau_levenshtein",
        family: Family::Base,
        core: |a, b| edit::damerau_levenshtein_restricted(a, b, |x, y| x == y) as f64,
        prepare: identity_prepare,
    },
    Algo {
        name: "hamming",
        family: Family::Base,
        core: |a, b| edit::hamming_distance(&[a, b], false, char_col_eq) as f64,
        prepare: identity_prepare,
    },
    Algo {
        name: "gotoh",
        family: Family::Gotoh,
        core: |a, b| edit::gotoh(a, b, 1.0, 0.4, |x, y| if x == y { 1.0 } else { 0.0 }),
        prepare: identity_prepare,
    },
    Algo {
        name: "needleman_wunsch",
        family: Family::NeedlemanWunsch,
        core: |a, b| edit::needleman_wunsch(a, b, 1.0, |x, y| if x == y { 1.0 } else { 0.0 }),
        prepare: identity_prepare,
    },
    Algo {
        name: "smith_waterman",
        family: Family::SmithWaterman,
        core: |a, b| edit::smith_waterman(a, b, 1.0, |x, y| if x == y { 1.0 } else { 0.0 }),
        prepare: identity_prepare,
    },
    Algo {
        name: "length",
        family: Family::Base,
        core: |a, b| length_distance(&[a.len(), b.len()]) as f64,
        prepare: identity_prepare,
    },
    Algo {
        name: "jaro",
        family: Family::SimMax1,
        core: |a, b| edit::jaro(a, b, |x, y| x == y),
        prepare: identity_prepare,
    },
    Algo {
        name: "jaro_winkler",
        family: Family::SimMax1,
        core: |a, b| edit::jaro_winkler(a, b, 0.1, false, true, |x, y| x == y),
        prepare: identity_prepare,
    },
    Algo {
        name: "strcmp95",
        family: Family::SimMax1,
        core: |a, b| {
            let au: Vec<u32> = a.iter().map(|c| *c as u32).collect();
            let bu: Vec<u32> = b.iter().map(|c| *c as u32).collect();
            edit::strcmp95(&au, &bu, false)
        },
        prepare: strcmp95_prepare,
    },
    Algo {
        name: "mlipns",
        family: Family::SimMax1,
        core: |a, b| edit::mlipns(a, b, 0.25, 2, char_col_eq),
        prepare: identity_prepare,
    },
];

fn find(name: &str) -> Option<&'static Algo> {
    ALGORITHMS.iter().find(|algo| algo.name == name)
}

fn metric_value(algo: &Algo, metric: &Metric, s1: &[char], s2: &[char]) -> f64 {
    let s1 = (algo.prepare)(s1);
    let s2 = (algo.prepare)(s2);
    let len1 = s1.len() as f64;
    let len2 = s2.len() as f64;
    let max_len = len1.max(len2);
    let min_len = len1.min(len2);
    // The similarity classes answer empty input with 0 before the kernel
    // runs (BaseSimilarity.quick_answer: "not all(sequences)").
    let empty = match algo.family {
        Family::SimMax1 | Family::SmithWaterman => s1.is_empty() || s2.is_empty(),
        _ => false,
    };
    // BaseSimilarity.quick_answer: identical sequences (incl. both empty) are
    // maximum immediately, but a single empty side is 0. Mirror that ordering
    // so ("","") scores maximum and ("abc","") scores 0.
    let both_empty = s1.is_empty() && s2.is_empty();
    let score = if empty {
        if both_empty {
            match algo.family {
                Family::SimMax1 => 1.0,
                Family::SmithWaterman => 0.0,
                _ => 0.0,
            }
        } else {
            0.0
        }
    } else {
        (algo.core)(&s1, &s2)
    };
    let (distance, similarity, normalized_distance, normalized_similarity) = match algo.family {
        Family::Base => {
            let distance = score;
            let similarity = max_len - distance;
            let nd = if max_len == 0.0 {
                0.0
            } else {
                distance / max_len
            };
            (distance, similarity, nd, 1.0 - nd)
        }
        Family::SimMax1 => {
            let similarity = score;
            let distance = 1.0 - similarity;
            let nd = distance;
            (distance, similarity, nd, 1.0 - nd)
        }
        Family::SmithWaterman => {
            let similarity = score;
            let distance = min_len - similarity;
            let nd = if min_len == 0.0 {
                0.0
            } else {
                distance / min_len
            };
            (distance, similarity, nd, 1.0 - nd)
        }
        Family::NeedlemanWunsch => {
            let maximum = max_len;
            let minimum = -max_len;
            let similarity = score;
            let distance = -similarity;
            if maximum == 0.0 {
                (distance, similarity, 0.0, 1.0)
            } else {
                let nd = (distance - minimum) / (maximum - minimum);
                let ns = (similarity - minimum) / (maximum * 2.0);
                (distance, similarity, nd, ns)
            }
        }
        Family::Gotoh => {
            let maximum = min_len;
            let minimum = -min_len;
            let similarity = score;
            let distance = -similarity;
            if maximum == 0.0 {
                (distance, similarity, 0.0, 1.0)
            } else {
                let nd = (distance - minimum) / (maximum - minimum);
                let ns = (similarity - minimum) / (maximum * 2.0);
                (distance, similarity, nd, ns)
            }
        }
    };
    match metric {
        Metric::Distance => distance,
        Metric::Similarity => similarity,
        Metric::NormalizedDistance => normalized_distance,
        Metric::NormalizedSimilarity => normalized_similarity,
    }
}

fn print_help() {
    println!(
        "tdc {}: string distance and similarity over the textdistance Rust kernels",
        VERSION
    );
    println!();
    println!("usage:");
    println!("  tdc list");
    println!("  tdc <metric> <algorithm> <s1> <s2>");
    println!();
    println!("metrics: distance, similarity, normalized_distance, normalized_similarity");
    println!();
    println!("algorithms:");
    for algo in ALGORITHMS {
        println!("  {}", algo.name);
    }
    println!();
    println!("example:");
    println!("  tdc distance levenshtein test text");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        exit(2);
    }
    match args[0].as_str() {
        "--version" | "-V" => {
            println!("tdc {}", VERSION);
        }
        "list" => {
            for algo in ALGORITHMS {
                println!("{}", algo.name);
            }
        }
        "help" | "--help" | "-h" => print_help(),
        metric_name
            if [
                "distance",
                "similarity",
                "normalized_distance",
                "normalized_similarity",
            ]
            .contains(&metric_name) =>
        {
            if args.len() != 4 {
                eprintln!("error: expected <metric> <algorithm> <s1> <s2>");
                exit(2);
            }
            let metric = match metric_name {
                "distance" => Metric::Distance,
                "similarity" => Metric::Similarity,
                "normalized_distance" => Metric::NormalizedDistance,
                _ => Metric::NormalizedSimilarity,
            };
            let Some(algo) = find(&args[1]) else {
                eprintln!("error: unknown algorithm '{}'", args[1]);
                eprintln!("run 'tdc list' to see supported algorithms");
                exit(2);
            };
            let s1 = chars(&args[2]);
            let s2 = chars(&args[3]);
            println!("{}", metric_value(algo, &metric, &s1, &s2));
        }
        other => {
            eprintln!("error: unknown command '{}'", other);
            eprintln!("run 'tdc --help' for usage");
            exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(name: &str, metric: &str, s1: &str, s2: &str) -> f64 {
        let algo = find(name).expect("known algorithm");
        let metric = match metric {
            "distance" => Metric::Distance,
            "similarity" => Metric::Similarity,
            "normalized_distance" => Metric::NormalizedDistance,
            _ => Metric::NormalizedSimilarity,
        };
        let a = chars(s1);
        let b = chars(s2);
        metric_value(algo, &metric, &a, &b)
    }

    fn assert_close(actual: f64, expected: f64) {
        let diff = (actual - expected).abs();
        assert!(diff < 1e-12, "actual {actual} != expected {expected}");
    }

    #[test]
    fn edit_family_matches_reference() {
        assert_eq!(value("levenshtein", "distance", "test", "text"), 1.0);
        assert_eq!(value("levenshtein", "distance", "", "abc"), 3.0);
        assert_eq!(
            value("damerau_levenshtein", "distance", "nelson", "neilsen"),
            2.0
        );
        assert_eq!(value("hamming", "distance", "test", "text"), 1.0);
        assert_eq!(value("length", "distance", "abc", "abcde"), 2.0);
        assert_eq!(value("levenshtein", "similarity", "ab", "ab"), 2.0);
    }

    #[test]
    fn similarity_family_matches_reference() {
        assert_close(
            value("jaro", "similarity", "MARTHA", "MARHTA"),
            0.9444444444444445,
        );
        assert_close(
            value("jaro_winkler", "normalized_similarity", "MARTHA", "MARHTA"),
            0.9611111111111111,
        );
        assert_close(
            value("strcmp95", "similarity", "MARTHA", "MARHTA"),
            0.9611111111111111,
        );
        assert_eq!(value("mlipns", "similarity", "MARTHA", "MARHTA"), 1.0);
        assert_eq!(value("jaro", "similarity", "abc", ""), 0.0);
        assert_eq!(value("mlipns", "distance", "tnw", ""), 1.0);
        assert_eq!(value("jaro", "similarity", "", ""), 1.0);
        assert_eq!(value("jaro", "distance", "", ""), 0.0);
        assert_eq!(value("jaro_winkler", "similarity", "", ""), 1.0);
        assert_eq!(value("strcmp95", "distance", "", ""), 0.0);
    }

    #[test]
    fn strcmp95_preprocesses_input() {
        assert_eq!(value("strcmp95", "similarity", "Martha", "MARTHA"), 1.0);
        assert_eq!(value("strcmp95", "similarity", "  MARTHA  ", "MARTHA"), 1.0);
        assert_close(
            value("strcmp95", "similarity", "  martha ", "M A RHTA"),
            0.875,
        );
        assert_eq!(value("strcmp95", "similarity", "   ", "x"), 0.0);
        assert_eq!(value("strcmp95", "similarity", "", "x"), 0.0);
    }

    #[test]
    fn alignment_families_match_reference() {
        assert_close(value("needleman_wunsch", "distance", "ab", "ab"), -2.0);
        assert_close(value("gotoh", "similarity", "MARTHA", "MARHTA"), 4.0);
        assert_eq!(
            value("smith_waterman", "distance", "fHIEY", "ymXVTotL"),
            5.0
        );
        assert_eq!(value("smith_waterman", "distance", "", "oe"), 0.0);
        assert_eq!(
            value("needleman_wunsch", "normalized_distance", "ab", "ab"),
            0.0
        );
        assert_eq!(
            value("needleman_wunsch", "normalized_similarity", "ab", "ab"),
            1.0
        );
    }
}
