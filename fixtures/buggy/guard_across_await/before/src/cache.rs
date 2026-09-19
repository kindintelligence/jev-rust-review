use std::sync::{Arc, Mutex};

pub struct Entry {
    pub value: String,
    pub hits: u64,
}

#[derive(Clone)]
pub struct Cache {
    inner: Arc<Mutex<Entry>>,
}

impl Cache {
    pub fn get(&self) -> String {
        let mut guard = self.inner.lock().unwrap();
        guard.hits += 1;
        guard.value.clone()
    }

    pub async fn refresh(&self) {
        let fresh = fetch_remote().await;
        let mut guard = self.inner.lock().unwrap();
        guard.value = fresh;
    }
}

async fn fetch_remote() -> String {
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    "fresh".to_string()
}
