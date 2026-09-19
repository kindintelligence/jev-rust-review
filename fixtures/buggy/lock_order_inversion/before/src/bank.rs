use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct Bank {
    pub(crate) accounts: Mutex<HashMap<u32, i64>>,
    pub(crate) ledger: Mutex<Vec<(u32, u32, i64)>>,
}

impl Bank {
    pub fn transfer(&self, from: u32, to: u32, amount: i64) {
        let mut accounts = self.accounts.lock().expect("accounts lock");
        let mut ledger = self.ledger.lock().expect("ledger lock");
        *accounts.entry(from).or_default() -= amount;
        *accounts.entry(to).or_default() += amount;
        ledger.push((from, to, amount));
    }
}
