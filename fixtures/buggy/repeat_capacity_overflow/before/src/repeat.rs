/// Returns `pattern` repeated `count` times.
pub fn repeat(pattern: &[u8], count: usize) -> Vec<u8> {
    let total = pattern
        .len()
        .checked_mul(count)
        .expect("capacity overflow");
    let mut out = Vec::with_capacity(total);
    for _ in 0..count {
        out.extend_from_slice(pattern);
    }
    out
}
