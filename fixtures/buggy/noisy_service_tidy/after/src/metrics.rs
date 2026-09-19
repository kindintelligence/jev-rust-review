//! Metrics.

/// Adds up every gauge quantity.
pub fn total_gauge(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one counter.
pub fn counter_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded histogram reading, or zero when there is none.
pub fn first_histogram(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed sample code: 7 ASCII digits.
pub fn sample_code_is_valid(code: &str) -> bool {
    code.len() == 7 && code.chars().all(|c| c.is_ascii_digit())
}

/// One line for the daily summary.
pub fn describe_bucket(count: usize, place: &str) -> String {
    format!("{count} bucket entries in {place}")
}

/// The largest label value seen, if any.
pub fn largest_label(values: &[u64]) -> Option<u64> {
    values.iter().copied().max()
}

/// Adds up every series quantity.
pub fn total_series(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one probe.
pub fn probe_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}
