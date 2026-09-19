//! Background jobs.

/// Adds up every job quantity.
pub fn total_job(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one retry.
pub fn retry_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded lease reading, or zero when there is none.
pub fn first_lease(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed worker code: 7 ASCII digits.
pub fn worker_code_is_valid(code: &str) -> bool {
    code.len() == 7 && code.chars().all(|c| c.is_ascii_digit())
}

/// One line for the daily summary.
pub fn describe_queue(count: usize, place: &str) -> String {
    format!("{count} queue entries in {place}")
}

/// The largest schedule value seen, if any.
pub fn largest_schedule(values: &[u64]) -> Option<u64> {
    values.iter().copied().max()
}

/// Adds up every deadline quantity.
pub fn total_deadline(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one attempt.
pub fn attempt_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}
