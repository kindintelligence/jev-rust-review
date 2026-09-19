use std::collections::HashMap;
use std::sync::Mutex;

pub struct Registry {
    users: Mutex<HashMap<String, u64>>,
}

impl Registry {
    pub fn register(&self, name: String, id: u64) -> Result<(), String> {
        let mut users = self.users.lock().unwrap();
        if users.contains_key(&name) {
            return Err(format!("{name} is taken"));
        }
        users.insert(name, id);
        Ok(())
    }
}
