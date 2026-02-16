use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

pub(super) struct RateLimiter {
    commands: Mutex<HashMap<u32, Vec<Instant>>>,
    max_commands: usize,
    window_secs: u64,
}

impl RateLimiter {
    pub(super) fn new(max_commands: usize, window_secs: u64) -> Self {
        Self {
            commands: Mutex::new(HashMap::new()),
            max_commands,
            window_secs,
        }
    }

    pub(super) fn check(&self, node_id: u32) -> bool {
        if self.max_commands == 0 {
            return true;
        }
        let mut map = self.commands.lock().unwrap();
        let now = Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        let timestamps = map.entry(node_id).or_default();
        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= self.max_commands {
            false
        } else {
            timestamps.push(now);
            true
        }
    }
}
