use std::{collections::HashMap, sync::RwLock};

pub struct Stream {
    // queue base storage stream
    queues: RwLock<HashMap<String, Vec<u8>>>,
}

impl Stream {
    pub fn new() -> Self {
        Stream {
            queues: RwLock::new(HashMap::new()),
        }
    }
    pub fn get(&self, key: &str, names: &[String]) -> Option<String> {
        let queues = self.queues.read().unwrap();
        if queues.contains_key(key) {
            Some(if names.is_empty() {
                String::from_utf8(queues.get(key).unwrap().clone()).unwrap()
            } else {
                let mut result = String::new();
                for name in names {
                    if let Some(value) = queues.get(name) {
                        result.push_str(&String::from_utf8(value.clone()).unwrap());
                        result.push('\n');
                    }
                }
                result
            })
        } else {
            return None;
        }
    }
    pub fn set(&self, key: String, value: Vec<u8>) {
        self.queues
            .write()
            .unwrap_or_else(|_| panic!("Failed to acquire write lock"))
            .insert(key, value);
    }
}
