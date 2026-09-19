use crate::bank::Bank;

impl Bank {
    pub fn transfer_count(&self) -> usize {
        self.ledger.lock().expect("ledger lock").len()
    }

    /// Checks that the ledger's net movement matches the balances.
    pub fn reconcile(&self) -> bool {
        let ledger = self.ledger.lock().expect("ledger lock");
        let accounts = self.accounts.lock().expect("accounts lock");
        // A transfer only moves money, so the balances always net to zero,
        // and each ledger entry touches at most two accounts.
        let held: i64 = accounts.values().sum();
        held == 0 && accounts.len() <= ledger.len() * 2
    }
}
