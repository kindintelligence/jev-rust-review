use crate::signal::Shutdown;

impl Shutdown {
    /// Returns once shutdown has been requested.
    pub async fn wait(&self) {
        if self.is_requested() {
            return;
        }
        self.wake.notified().await;
    }
}
