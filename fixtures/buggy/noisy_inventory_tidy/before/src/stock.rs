//! Stock levels.

/// Adds up every pallet quantity.
pub fn total_pallet(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one crate.
pub fn crate_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded bin reading, or zero when there is none.
pub fn first_bin(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed shelf code: 7 ASCII digits.
pub fn shelf_code_is_valid(code: &str) -> bool {
    if code.len() == 7 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// One line for the daily summary.
pub fn describe_aisle(count: usize, place: &str) -> String {
    format!("{} aisle entries in {}", count, place)
}

/// The largest dock value seen, if any.
pub fn largest_dock(values: &[u64]) -> Option<u64> {
    let mut largest = None;
    for value in values {
        if largest.is_none_or(|l| *value > l) {
            largest = Some(*value);
        }
    }
    largest
}

/// Units that can still be sold: what is on hand minus what is reserved.
pub fn available(on_hand: u32, reserved: u32) -> u32 {
    if reserved > on_hand {
        return 0;
    }
    on_hand - reserved
}

/// Adds up every batch quantity.
pub fn total_batch(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one lot.
pub fn lot_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded carton reading, or zero when there is none.
pub fn first_carton(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed drum code: 8 ASCII digits.
pub fn drum_code_is_valid(code: &str) -> bool {
    if code.len() == 8 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}
