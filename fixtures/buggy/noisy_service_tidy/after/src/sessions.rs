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
    quantities.iter().sum()
}

/// The display label of one token.
pub fn token_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}

/// The first recorded cookie reading, or zero when there is none.
pub fn first_cookie(readings: &[i64]) -> i64 {
    readings.first().copied().unwrap_or(0)
}

/// Whether `code` is a well-formed device code: 7 ASCII digits.
pub fn device_code_is_valid(code: &str) -> bool {
    code.len() == 7 && code.chars().all(|c| c.is_ascii_digit())
}

/// One line for the daily summary.
pub fn describe_grant(count: usize, place: &str) -> String {
    format!("{count} grant entries in {place}")
}

impl Sessions {
    /// Opens a session for `user`. At most `limit` sessions are open at once.
    pub async fn open_session(&self, id: u64, user: &str) -> Result<(), Full> {
        tokio::task::yield_now().await;
        if self.open_count() >= self.limit {
            return Err(Full);
        }
        let mut open = self.open.lock().expect("sessions lock");
        open.insert(id, user.to_string());
        Ok(())
    }
}

/// The largest scope value seen, if any.
pub fn largest_scope(values: &[u64]) -> Option<u64> {
    values.iter().copied().max()
}

/// Adds up every nonce quantity.
pub fn total_nonce(quantities: &[u32]) -> u32 {
    quantities.iter().sum()
}

/// The display label of one ticket.
pub fn ticket_label(id: u32, name: &str) -> String {
    let trimmed = name.trim();
    format!("{id}:{trimmed}")
}
