use std::sync::Mutex;

pub struct Counter {
    count: Mutex<u64>,
}

impl Counter {
    pub fn get(&self) -> u64 {
        *self.count.lock().unwrap()
    }

    pub async fn bump(&self) {
        let snapshot = {
            let mut n = self.count.lock().unwrap();
            *n += 1;
            *n
        };
        notify(snapshot).await;
    }
}

async fn notify(value: u64) {
    let _ = value;
}
