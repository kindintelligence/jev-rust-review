//! Orders and shipments.

/// Adds up every order quantity.
pub fn total_order(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one parcel.
pub fn parcel_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded pickup reading, or zero when there is none.
pub fn first_pickup(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed return_slip code: 7 ASCII digits.
pub fn return_slip_code_is_valid(code: &str) -> bool {
    if code.len() == 7 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// One line for the daily summary.
pub fn describe_backorder(count: usize, place: &str) -> String {
    format!("{} backorder entries in {}", count, place)
}

/// The largest shipment value seen, if any.
pub fn largest_shipment(values: &[u64]) -> Option<u64> {
    let mut largest = None;
    for value in values {
        if largest.is_none_or(|l| *value > l) {
            largest = Some(*value);
        }
    }
    largest
}

/// Adds up every manifest quantity.
pub fn total_manifest(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one waybill.
pub fn waybill_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded consignment reading, or zero when there is none.
pub fn first_consignment(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed route code: 8 ASCII digits.
pub fn route_code_is_valid(code: &str) -> bool {
    if code.len() == 8 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}
