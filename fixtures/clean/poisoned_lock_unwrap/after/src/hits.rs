use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct Hits {
    by_page: Mutex<HashMap<String, u64>>,
}

impl Hits {
    pub fn record(&self, page: &str) {
        let mut by_page = self.by_page.lock().unwrap();
        *by_page.entry(page.to_owned()).or_insert(0) += 1;
    }

    /// How often `page` has been recorded.
    pub fn count(&self, page: &str) -> u64 {
        let by_page = self.by_page.lock().unwrap();
        by_page.get(page).copied().unwrap_or(0)
    }
}
