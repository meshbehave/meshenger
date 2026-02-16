pub(super) struct DashboardNotifier {
    tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl DashboardNotifier {
    pub(super) fn new() -> Self {
        Self { tx: None }
    }

    pub(super) fn set_sender(&mut self, tx: tokio::sync::broadcast::Sender<()>) {
        self.tx = Some(tx);
    }

    pub(super) fn notify(&self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(());
        }
    }
}
