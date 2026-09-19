/// Appends every item of `items` to `out`.
pub fn append<I: IntoIterator<Item = u32>>(out: &mut Vec<u32>, items: I) {
    for item in items {
        out.push(item);
    }
}
