use crate::sink::{forward, Event};
use tokio::sync::mpsc::{Receiver, Sender};

pub async fn relay(mut rx: Receiver<Event>, tx: Sender<Event>) {
    while let Some(event) = rx.recv().await {
        forward(&tx, event).await;
    }
}
