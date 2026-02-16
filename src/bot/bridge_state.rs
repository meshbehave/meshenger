use crate::bridge::{MeshMessageSender, OutgoingMessageReceiver};

pub(super) struct BridgeState {
    tx: Option<MeshMessageSender>,
    rx: Option<tokio::sync::Mutex<OutgoingMessageReceiver>>,
}

impl BridgeState {
    pub(super) fn new() -> Self {
        Self { tx: None, rx: None }
    }

    pub(super) fn set_channels(&mut self, tx: MeshMessageSender, rx: OutgoingMessageReceiver) {
        self.tx = Some(tx);
        self.rx = Some(tokio::sync::Mutex::new(rx));
    }

    pub(super) fn tx(&self) -> Option<&MeshMessageSender> {
        self.tx.as_ref()
    }

    pub(super) fn rx(&self) -> Option<&tokio::sync::Mutex<OutgoingMessageReceiver>> {
        self.rx.as_ref()
    }
}
