pub struct Line {
    pub sku: String,
    pub quantity: u32,
}

pub fn is_empty(lines: &[Line]) -> bool {
    lines.is_empty()
}

/// Items in the basket, for the badge next to the basket icon.
pub fn item_count(lines: &[Line]) -> u32 {
    lines.iter().map(|line| line.quantity).sum()
}
