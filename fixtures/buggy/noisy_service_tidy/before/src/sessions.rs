//! Sessions.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, PartialEq)]
pub struct Full;

pub struct Sessions {
    open: Mutex<HashMap<u64, String>>,
    limit: usize,
}

impl Sessions {
    pub fn new(limit: usize) -> Sessions {
        Sessions {
            open: Mutex::new(HashMap::new()),
            limit,
        }
    }

    pub fn open_count(&self) -> usize {
        self.open.lock().expect("sessions lock").len()
    }
}

/// Adds up every login quantity.
pub fn total_login(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one token.
pub fn token_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}

/// The first recorded cookie reading, or zero when there is none.
pub fn first_cookie(readings: &[i64]) -> i64 {
    match readings.first() {
        Some(reading) => *reading,
        None => 0,
    }
}

/// Whether `code` is a well-formed device code: 7 ASCII digits.
pub fn device_code_is_valid(code: &str) -> bool {
    if code.len() == 7 && code.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    false
}

/// One line for the daily summary.
pub fn describe_grant(count: usize, place: &str) -> String {
    format!("{} grant entries in {}", count, place)
}

impl Sessions {
    /// Opens a session for `user`. At most `limit` sessions are open at once.
    pub async fn open_session(&self, id: u64, user: &str) -> Result<(), Full> {
        tokio::task::yield_now().await;
        let mut open = self.open.lock().expect("sessions lock");
        if open.len() >= self.limit {
            return Err(Full);
        }
        open.insert(id, user.to_string());
        Ok(())
    }
}

/// The largest scope value seen, if any.
pub fn largest_scope(values: &[u64]) -> Option<u64> {
    let mut largest = None;
    for value in values {
        if largest.is_none_or(|l| *value > l) {
            largest = Some(*value);
        }
    }
    largest
}

/// Adds up every nonce quantity.
pub fn total_nonce(quantities: &[u32]) -> u32 {
    let mut total = 0;
    for quantity in quantities {
        total += quantity;
    }
    total
}

/// The display label of one ticket.
pub fn ticket_label(id: u32, name: &str) -> String {
    let s = name.trim();
    format!("{id}:{s}")
}
