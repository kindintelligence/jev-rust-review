use crate::sink::{forward, Event};
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::interval;

pub async fn relay(mut rx: Receiver<Event>, tx: Sender<Event>) {
    let mut heartbeat = interval(Duration::from_secs(5));
    while let Some(event) = rx.recv().await {
        tokio::select! {
            _ = heartbeat.tick() => {
                eprintln!("relay is alive");
            }
            () = forward(&tx, event) => {}
        }
    }
}
