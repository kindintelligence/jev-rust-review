//! Background jobs.

/// Adds up every job quantity.
pub fn total_job(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one retry.
pub fn retry_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded lease reading, or zero when there is none.
pub fn first_lease(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed worker code: 7 ASCII digits.
pub fn worker_code_is_valid(code: &str) -> bool {
    if code.len() == 7 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// One line for the daily summary.
pub fn describe_queue(count: usize, place: &str) -> String {
    format!("{} queue entries in {}", count, place)
}

/// The largest schedule value seen, if any.
pub fn largest_schedule(values: &[u64]) -> Option<u64> {
    let mut largest = None;
    for value in values {
        if largest.is_none_or(|l| *value > l) {
            largest = Some(*value);
        }
    }
    largest
}

/// Adds up every deadline quantity.
pub fn total_deadline(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one attempt.
pub fn attempt_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}
