//! Simple-family kernels.
//!
//! Prefix/postfix common-sequence helpers and the length measure. Identity and
//! Matrix are trivial dict/int logic and live entirely in the Python adapter.

/// Length of the longest shared prefix across all sequences.
pub fn common_prefix<T, F>(seqs: &[Vec<T>], test: F) -> Vec<&T>
where
    F: Fn(&T, &T) -> bool,
{
    if seqs.is_empty() {
        return Vec::new();
    }
    let first = &seqs[0];
    let mut result = Vec::new();
    'outer: for (i, first_item) in first.iter().enumerate() {
        for s in &seqs[1..] {
            match s.get(i) {
                Some(e) => {
                    if !test(first_item, e) {
                        break 'outer;
                    }
                }
                None => break 'outer,
            }
        }
        result.push(first_item);
    }
    result
}

/// Difference between the longest and the shortest sequence length.
pub fn length_distance(lengths: &[usize]) -> usize {
    let max = lengths.iter().copied().max().unwrap_or(0);
    let min = lengths.iter().copied().min().unwrap_or(0);
    max - min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_basics() {
        let a: Vec<char> = "abcde".chars().collect();
        let b: Vec<char> = "abxyz".chars().collect();
        let seqs = [a, b];
        let result = common_prefix(&seqs, |x: &char, y: &char| x == y);
        let s: String = result.into_iter().copied().collect();
        assert_eq!(s, "ab");
    }
}
