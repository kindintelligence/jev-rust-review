use std::collections::HashMap;
use std::sync::Mutex;

pub struct Registry {
    users: Mutex<HashMap<String, u64>>,
}

impl Registry {
    pub fn register(&self, name: String, id: u64) -> Result<(), String> {
        if self.users.lock().unwrap().contains_key(&name) {
            return Err(format!("{name} is taken"));
        }
        audit(&name);
        self.users.lock().unwrap().insert(name, id);
        Ok(())
    }
}

fn audit(name: &str) {
    eprintln!("register {name}");
}
