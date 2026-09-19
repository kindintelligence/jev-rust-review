use std::sync::Mutex;

pub struct Counter {
    count: Mutex<u64>,
}

impl Counter {
    pub fn get(&self) -> u64 {
        *self.count.lock().unwrap()
    }
}
