use crate::signal::Shutdown;

impl Shutdown {
    /// Returns once shutdown has been requested.
    pub async fn wait(&self) {
        let notified = self.wake.notified();
        tokio::pin!(notified);
        // Register before the check, so a trigger after it still wakes us.
        notified.as_mut().enable();
        if self.is_requested() {
            return;
        }
        notified.await;
    }
}
