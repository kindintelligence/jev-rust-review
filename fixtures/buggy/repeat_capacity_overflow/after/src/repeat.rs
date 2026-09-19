/// Returns `pattern` repeated `count` times.
pub fn repeat(pattern: &[u8], count: usize) -> Vec<u8> {
    let total = pattern.len() * count;
    let mut out: Vec<u8> = Vec::with_capacity(total);
    // SAFETY: `out` has capacity for `total` bytes, every byte below `total`
    // is written before `set_len`, and the source and target do not overlap.
    unsafe {
        let base = out.as_mut_ptr();
        for i in 0..count {
            std::ptr::copy_nonoverlapping(
                pattern.as_ptr(),
                base.add(i * pattern.len()),
                pattern.len(),
            );
        }
        out.set_len(total);
    }
    out
}
