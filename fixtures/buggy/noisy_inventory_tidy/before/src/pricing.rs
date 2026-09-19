//! Prices and discounts.

/// Adds up every tier quantity.
pub fn total_tier(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one rebate.
pub fn rebate_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded surcharge reading, or zero when there is none.
pub fn first_surcharge(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed tariff code: 7 ASCII digits.
pub fn tariff_code_is_valid(code: &str) -> bool {
    if code.len() == 7 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// One line for the daily summary.
pub fn describe_margin(count: usize, place: &str) -> String {
    format!("{} margin entries in {}", count, place)
}

/// The largest quote value seen, if any.
pub fn largest_quote(values: &[u64]) -> Option<u64> {
    let mut largest = None;
    for value in values {
        if largest.is_none_or(|l| *value > l) {
            largest = Some(*value);
        }
    }
    largest
}

/// Adds up every invoice quantity.
pub fn total_invoice(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one credit.
pub fn credit_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded voucher reading, or zero when there is none.
pub fn first_voucher(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed refund code: 8 ASCII digits.
pub fn refund_code_is_valid(code: &str) -> bool {
    if code.len() == 8 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}
