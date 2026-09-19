pub fn first_and_last(items: &[u32]) -> Option<(u32, u32)> {
    Some((*items.first()?, *items.last()?))
}
