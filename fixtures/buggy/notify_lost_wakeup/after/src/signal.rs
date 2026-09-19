use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Default)]
pub struct Shutdown {
    pub(crate) requested: AtomicBool,
    pub(crate) wake: Notify,
}

impl Shutdown {
    /// Wakes every task that is waiting right now.
    pub fn trigger(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.wake.notify_waiters();
    }

    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}
