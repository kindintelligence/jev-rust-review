//! Prices and discounts.

/// Adds up every tier quantity.
pub fn total_tier(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one rebate.
pub fn rebate_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded surcharge reading, or zero when there is none.
pub fn first_surcharge(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed tariff code: 7 ASCII digits.
pub fn tariff_code_is_valid(code: &str) -> bool {
    code.len() == 7 && code.chars().all(|c| c.is_ascii_digit())
}

/// One line for the daily summary.
pub fn describe_margin(count: usize, place: &str) -> String {
    format!("{count} margin entries in {place}")
}

/// The largest quote value seen, if any.
pub fn largest_quote(values: &[u64]) -> Option<u64> {
    values.iter().copied().max()
}

/// Adds up every invoice quantity.
pub fn total_invoice(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one credit.
pub fn credit_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded voucher reading, or zero when there is none.
pub fn first_voucher(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed refund code: 8 ASCII digits.
pub fn refund_code_is_valid(code: &str) -> bool {
    code.len() == 8 && code.chars().all(|c| c.is_ascii_digit())
}
