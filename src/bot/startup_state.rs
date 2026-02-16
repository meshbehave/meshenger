use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::message::MeshEvent;

pub(super) struct StartupState {
    connected_at: Mutex<Option<Instant>>,
    deferred_events: Mutex<Vec<MeshEvent>>,
}

impl StartupState {
    pub(super) fn new() -> Self {
        Self {
            connected_at: Mutex::new(None),
            deferred_events: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn mark_connected_and_reset(&self) {
        *self.connected_at.lock().unwrap() = Some(Instant::now());
        self.deferred_events.lock().unwrap().clear();
    }

    pub(super) fn in_grace_period(&self, grace_secs: u64) -> bool {
        self.connected_at
            .lock()
            .unwrap()
            .map(|t| t.elapsed() < Duration::from_secs(grace_secs))
            .unwrap_or(false)
    }

    pub(super) fn defer_event(&self, event: MeshEvent) {
        self.deferred_events.lock().unwrap().push(event);
    }

    pub(super) fn take_deferred(&self) -> Vec<MeshEvent> {
        let mut deferred = self.deferred_events.lock().unwrap();
        std::mem::take(&mut *deferred)
    }
}
