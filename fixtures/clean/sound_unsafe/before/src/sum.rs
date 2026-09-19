pub fn sum_prefix(values: &[u64], n: usize) -> u64 {
    values.iter().take(n).sum()
}
