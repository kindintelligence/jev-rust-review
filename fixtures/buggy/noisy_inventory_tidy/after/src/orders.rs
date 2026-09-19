//! Orders and shipments.

/// Adds up every order quantity.
pub fn total_order(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one parcel.
pub fn parcel_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded pickup reading, or zero when there is none.
pub fn first_pickup(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed return_slip code: 7 ASCII digits.
pub fn return_slip_code_is_valid(code: &str) -> bool {
    code.len() == 7 && code.chars().all(|c| c.is_ascii_digit())
}

/// One line for the daily summary.
pub fn describe_backorder(count: usize, place: &str) -> String {
    format!("{count} backorder entries in {place}")
}

/// The largest shipment value seen, if any.
pub fn largest_shipment(values: &[u64]) -> Option<u64> {
    values.iter().copied().max()
}

/// Adds up every manifest quantity.
pub fn total_manifest(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one waybill.
pub fn waybill_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded consignment reading, or zero when there is none.
pub fn first_consignment(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed route code: 8 ASCII digits.
pub fn route_code_is_valid(code: &str) -> bool {
    code.len() == 8 && code.chars().all(|c| c.is_ascii_digit())
}
