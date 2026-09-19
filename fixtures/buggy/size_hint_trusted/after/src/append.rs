fn room_for<I: Iterator>(iter: &I) -> usize {
    let (lower, upper) = iter.size_hint();
    upper.unwrap_or(lower)
}

/// Appends every item of `items` to `out`.
pub fn append<I: IntoIterator<Item = u32>>(out: &mut Vec<u32>, items: I) {
    let iter = items.into_iter();
    let room = room_for(&iter);
    out.reserve(room);
    let mut len = out.len();
    // SAFETY: `reserve` made room for `room` more elements after `len`, and
    // `set_len` runs only after those elements are written.
    unsafe {
        let base = out.as_mut_ptr();
        for item in iter {
            base.add(len).write(item);
            len += 1;
        }
        out.set_len(len);
    }
}
