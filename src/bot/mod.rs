use std::sync::atomic::{AtomicU32, AtomicUsize};
use std::sync::Arc;

use crate::bridge::{MeshMessageSender, OutgoingMessageReceiver};
use crate::config::Config;
use crate::db::Db;
use crate::module::ModuleRegistry;

mod bridge_state;
mod command_handler;
mod dashboard_notifier;
mod events;
mod incoming;
mod outgoing;
mod rate_limit;
mod runtime;
mod startup_state;
mod traceroute_state;

#[cfg(test)]
mod tests;

use bridge_state::BridgeState;
use dashboard_notifier::DashboardNotifier;
use outgoing::{OutgoingKind, OutgoingMeshMessage, OutgoingQueue};
use rate_limit::RateLimiter;
use startup_state::StartupState;
use traceroute_state::TracerouteState;

pub struct Bot {
    config: Arc<Config>,
    db: Arc<Db>,
    registry: Arc<ModuleRegistry>,
    rate_limiter: RateLimiter,
    /// Tracks startup timing + deferred events for grace-period handling.
    startup_state: StartupState,
    /// Channel state for bridge in/out communication.
    bridge: BridgeState,
    /// Outgoing message queue drained by the event loop timer
    outgoing: OutgoingQueue,
    /// SSE broadcast sender for real-time dashboard updates
    notifier: DashboardNotifier,
    /// Last traceroute probe send time per target node
    traceroute: TracerouteState,
    /// Node ID of the connected local node (0 until MyInfo is received)
    local_node_id: Arc<AtomicU32>,
}

impl Bot {
    pub(super) fn traceroute_session_key(
        src_node: u32,
        dst_node: Option<u32>,
        request_mesh_id: u32,
    ) -> String {
        let dst = dst_node
            .map(|n| format!("{:08x}", n))
            .unwrap_or_else(|| "broadcast".to_string());
        format!("req:{:08x}:{}:{:08x}", src_node, dst, request_mesh_id)
    }

    pub fn new(config: Arc<Config>, db: Arc<Db>, registry: ModuleRegistry) -> Self {
        let rate_limiter = RateLimiter::new(
            config.bot.rate_limit_commands,
            config.bot.rate_limit_window_secs,
        );
        Self {
            config,
            db,
            registry: Arc::new(registry),
            rate_limiter,
            startup_state: StartupState::new(),
            bridge: BridgeState::new(),
            outgoing: OutgoingQueue::new(),
            notifier: DashboardNotifier::new(),
            traceroute: TracerouteState::new(),
            local_node_id: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Returns a shared handle to the queue depth counter (for the dashboard).
    pub fn queue_depth(&self) -> Arc<AtomicUsize> {
        self.outgoing.depth_handle()
    }

    /// Returns the currently connected local node ID handle (0 until connected).
    pub fn local_node_id(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.local_node_id)
    }

    /// Set bridge channels for communication with external platforms.
    pub fn with_bridge_channels(
        mut self,
        bridge_tx: MeshMessageSender,
        bridge_rx: OutgoingMessageReceiver,
    ) -> Self {
        self.bridge.set_channels(bridge_tx, bridge_rx);
        self
    }

    /// Set the SSE broadcast sender for real-time dashboard notifications.
    pub fn with_sse_sender(mut self, tx: tokio::sync::broadcast::Sender<()>) -> Self {
        self.notifier.set_sender(tx);
        self
    }

    /// Notify the dashboard that data has changed (non-blocking, best-effort).
    fn notify_dashboard(&self) {
        self.notifier.notify();
    }

    fn queue_message(&self, msg: OutgoingMeshMessage) {
        self.outgoing.push(msg);
    }
}
