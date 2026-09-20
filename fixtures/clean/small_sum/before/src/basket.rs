pub struct Line {
    pub sku: String,
    pub quantity: u32,
}

pub fn is_empty(lines: &[Line]) -> bool {
    lines.is_empty()
}
