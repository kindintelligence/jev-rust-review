/// Sums the first `n` values, or all of them if there are fewer.
pub fn sum_prefix(values: &[u64], n: usize) -> u64 {
    let n = n.min(values.len());
    let mut total = 0u64;
    for i in 0..n {
        // SAFETY: `n <= values.len()` and `i < n`, so `i` is in bounds.
        total = total.wrapping_add(unsafe { *values.get_unchecked(i) });
    }
    total
}
