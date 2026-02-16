use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) struct TracerouteState {
    last_sent: Mutex<HashMap<u32, Instant>>,
}

impl TracerouteState {
    pub(super) fn new() -> Self {
        Self {
            last_sent: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn can_send(&self, target: u32, cooldown_secs: u64) -> bool {
        let last_sent = self.last_sent.lock().unwrap();
        if let Some(last) = last_sent.get(&target) {
            return last.elapsed() >= Duration::from_secs(cooldown_secs);
        }
        true
    }

    pub(super) fn mark_sent(&self, target: u32) {
        self.last_sent
            .lock()
            .unwrap()
            .insert(target, Instant::now());
    }
}
