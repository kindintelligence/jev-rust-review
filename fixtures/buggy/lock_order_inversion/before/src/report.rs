use crate::bank::Bank;

impl Bank {
    pub fn transfer_count(&self) -> usize {
        self.ledger.lock().expect("ledger lock").len()
    }
}
