use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Event {
    pub id: u64,
    pub body: String,
}

/// Delivers one event downstream, waiting while the channel is full.
pub async fn forward(tx: &Sender<Event>, event: Event) {
    if tx.send(event).await.is_err() {
        eprintln!("downstream closed");
    }
}
