use meshtastic::api::StreamApi;
use meshtastic::packet::{PacketDestination, PacketRouter};
use meshtastic::protobufs::{self, from_radio};
use meshtastic::types::{MeshChannel, NodeId};
use meshtastic::utils;
use meshtastic::utils::stream::build_tcp_stream;
use rand::Rng;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc::UnboundedReceiver;

use super::*;

#[derive(Debug)]
pub(super) struct RouterError(String);

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RouterError {}

pub(super) struct BotPacketRouter {
    node_id: u32,
}

impl PacketRouter<(), RouterError> for BotPacketRouter {
    fn handle_packet_from_radio(
        &mut self,
        _packet: protobufs::FromRadio,
    ) -> Result<(), RouterError> {
        Ok(())
    }

    fn handle_mesh_packet(&mut self, _packet: protobufs::MeshPacket) -> Result<(), RouterError> {
        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::from(self.node_id)
    }
}

#[derive(Debug, Clone, Copy)]
struct ProbeSelection {
    target: Option<u32>,
    cooldown_skipped: usize,
    queried_limits: usize,
    had_candidates: bool,
}

fn select_probe_target_adaptive<F, E, G>(
    limits: &[usize],
    mut fetch_candidates: F,
    mut can_send: G,
) -> Result<ProbeSelection, E>
where
    F: FnMut(usize) -> Result<Vec<u32>, E>,
    G: FnMut(u32) -> bool,
{
    let mut seen = HashSet::new();
    let mut cooldown_skipped = 0usize;
    let mut queried_limits = 0usize;
    let mut had_candidates = false;

    for &limit in limits {
        let candidates = fetch_candidates(limit)?;
        queried_limits += 1;
        if candidates.is_empty() {
            break;
        }
        had_candidates = true;

        for node_id in candidates.iter().copied() {
            if !seen.insert(node_id) {
                continue;
            }
            if can_send(node_id) {
                return Ok(ProbeSelection {
                    target: Some(node_id),
                    cooldown_skipped,
                    queried_limits,
                    had_candidates,
                });
            }
            cooldown_skipped += 1;
        }

        if candidates.len() < limit {
            break;
        }
    }

    Ok(ProbeSelection {
        target: None,
        cooldown_skipped,
        queried_limits,
        had_candidates,
    })
}

impl Bot {
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let reconnect_delay =
            std::time::Duration::from_secs(self.config.connection.reconnect_delay_secs);

        loop {
            match self.connect_and_run().await {
                Ok(()) => {
                    log::warn!("Connection closed cleanly");
                }
                Err(e) => {
                    log::error!("Connection error: {}", e);
                }
            }

            log::info!("Reconnecting in {} seconds...", reconnect_delay.as_secs());
            tokio::time::sleep(reconnect_delay).await;
        }
    }

    async fn connect_and_run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let address = &self.config.connection.address;
        log::info!("Connecting to meshtastic node at {}...", address);

        let tcp_stream = build_tcp_stream(address.to_string()).await?;
        let (mut packet_rx, stream_api) = StreamApi::new().connect(tcp_stream).await;

        let config_id = utils::generate_rand_id();
        let configured_api = stream_api.configure(config_id).await?;

        log::info!("Connected and configured (config_id={})", config_id);

        let my_node_id = self.wait_for_my_node_id(&mut packet_rx).await?;
        self.local_node_id.store(my_node_id, Ordering::Relaxed);
        log::info!("Bot node ID: !{:08x}", my_node_id);

        let mut router = BotPacketRouter {
            node_id: my_node_id,
        };

        self.event_loop(my_node_id, &mut packet_rx, configured_api, &mut router)
            .await
    }

    async fn wait_for_my_node_id(
        &self,
        packet_rx: &mut UnboundedReceiver<protobufs::FromRadio>,
    ) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        while let Some(packet) = packet_rx.recv().await {
            if let Some(from_radio::PayloadVariant::MyInfo(my_info)) = packet.payload_variant {
                return Ok(my_info.my_node_num);
            }
        }
        Err("Channel closed before receiving MyNodeInfo".into())
    }

    async fn event_loop(
        &self,
        my_node_id: u32,
        packet_rx: &mut UnboundedReceiver<protobufs::FromRadio>,
        mut api: meshtastic::api::ConnectedStreamApi,
        router: &mut BotPacketRouter,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!("Entering event loop...");
        self.startup_state.mark_connected_and_reset();

        // Timer to dispatch deferred events after the grace period
        let grace_period = std::time::Duration::from_secs(self.config.bot.startup_grace_secs);
        let grace_timer = tokio::time::sleep(grace_period);
        tokio::pin!(grace_timer);
        let mut grace_period_done = false;

        // Timer for draining the outgoing message queue
        let send_delay = std::time::Duration::from_millis(self.config.bot.send_delay_ms);
        let send_timer = tokio::time::sleep(send_delay);
        tokio::pin!(send_timer);

        let traceroute_enabled = self.config.traceroute_probe.enabled;
        let traceroute_base_interval =
            std::time::Duration::from_secs(self.config.traceroute_probe.interval_secs.max(60));
        let traceroute_jitter_pct =
            sanitize_traceroute_jitter_pct(self.config.traceroute_probe.interval_jitter_pct);
        let traceroute_timer = tokio::time::sleep(next_traceroute_interval(
            traceroute_base_interval,
            traceroute_jitter_pct,
        ));
        tokio::pin!(traceroute_timer);

        let stale_node_max_age = std::time::Duration::from_secs(7 * 24 * 60 * 60);
        let stale_node_purge_interval = std::time::Duration::from_secs(60 * 60);
        let stale_node_purge_timer = tokio::time::sleep(stale_node_purge_interval);
        tokio::pin!(stale_node_purge_timer);

        // PRAGMA optimize: run every 6 hours to keep query planner stats fresh.
        let optimize_interval = std::time::Duration::from_secs(6 * 60 * 60);
        let optimize_timer = tokio::time::sleep(optimize_interval);
        tokio::pin!(optimize_timer);

        self.purge_stale_nodes(stale_node_max_age);

        // Bridge active flag: set to false when the bridge channel closes.
        let mut bridge_active = self.bridge.rx().is_some();

        loop {
            let queue_has_messages = !self.outgoing.is_empty();

            tokio::select! {
                // Handle messages from bridges; disabled when no bridge or after channel close.
                // The async block acquires the lock only for the duration of recv(), so it is
                // dropped (not held) when any other branch wins the select.
                msg = async { self.bridge.rx().unwrap().lock().await.recv().await },
                    if bridge_active =>
                {
                    match msg {
                        Some(msg) => self.handle_bridge_message(my_node_id, msg),
                        None => {
                            bridge_active = false;
                            log::warn!("Bridge outgoing channel closed; disabling bridge receive path");
                        }
                    }
                }

                // Handle packets from mesh
                packet = packet_rx.recv() => {
                    match packet {
                        Some(p) => self.process_radio_packet(my_node_id, p).await,
                        None => {
                            log::warn!("Packet channel closed, exiting event loop");
                            return Ok(());
                        }
                    }
                }

                // Dispatch deferred events after grace period
                _ = &mut grace_timer, if !grace_period_done => {
                    grace_period_done = true;
                    self.dispatch_deferred_events(my_node_id).await;
                }

                // Drain outgoing message queue
                _ = &mut send_timer, if queue_has_messages => {
                    self.send_next_queued_message(&mut api, router).await;
                    self.notify_dashboard();
                    send_timer.as_mut().reset(tokio::time::Instant::now() + send_delay);
                }

                // Periodic traceroute probe
                _ = &mut traceroute_timer, if traceroute_enabled => {
                    self.maybe_queue_traceroute_probe(my_node_id);
                    traceroute_timer.as_mut().reset(
                        tokio::time::Instant::now()
                            + next_traceroute_interval(traceroute_base_interval, traceroute_jitter_pct),
                    );
                }

                // Periodic stale node purge
                _ = &mut stale_node_purge_timer => {
                    self.purge_stale_nodes(stale_node_max_age);
                    stale_node_purge_timer.as_mut().reset(tokio::time::Instant::now() + stale_node_purge_interval);
                }

                // Periodic PRAGMA optimize
                _ = &mut optimize_timer => {
                    if let Err(e) = self.db.optimize() {
                        log::warn!("PRAGMA optimize failed: {}", e);
                    }
                    optimize_timer.as_mut().reset(tokio::time::Instant::now() + optimize_interval);
                }
            }
        }
    }

    fn purge_stale_nodes(&self, max_age: std::time::Duration) {
        match self.db.purge_nodes_not_seen_within(max_age.as_secs()) {
            Ok(purged) if purged > 0 => {
                let days = max_age.as_secs() / (24 * 60 * 60);
                log::info!(
                    "Purged {} stale node(s) not seen in over {} day(s)",
                    purged,
                    days
                );
                self.notify_dashboard();
            }
            Ok(_) => {}
            Err(e) => {
                log::error!("Failed to purge stale nodes: {}", e);
            }
        }
    }

    fn maybe_queue_traceroute_probe(&self, my_node_id: u32) {
        let cfg = &self.config.traceroute_probe;
        if !cfg.enabled {
            log::info!("Traceroute probe skipped: feature disabled");
            return;
        }

        let limits = [10usize, 25, 50, 100];
        let selection = match select_probe_target_adaptive(
            &limits,
            |limit| {
                self.db.recent_rf_nodes_missing_hops(
                    cfg.recent_seen_within_secs,
                    Some(my_node_id),
                    limit,
                )
            },
            |node_id| {
                let can_send = self
                    .traceroute
                    .can_send(node_id, cfg.per_node_cooldown_secs);
                if !can_send {
                    log::trace!(
                        "Traceroute probe candidate !{:08x} skipped due to cooldown ({}s)",
                        node_id,
                        cfg.per_node_cooldown_secs
                    );
                }
                can_send
            },
        ) {
            Ok(sel) => sel,
            Err(e) => {
                log::error!("Traceroute probe candidate query failed: {}", e);
                return;
            }
        };

        if !selection.had_candidates {
            log::info!(
                "Traceroute probe skipped: no eligible RF node missing hop data within last {}s",
                cfg.recent_seen_within_secs
            );
            return;
        }

        let target = match selection.target {
            Some(node_id) => node_id,
            None => {
                log::info!(
                    "Traceroute probe skipped: all eligible candidates are in cooldown (checked={}, windows={})",
                    selection.cooldown_skipped,
                    selection.queried_limits
                );
                return;
            }
        };

        if selection.cooldown_skipped > 0 {
            log::info!(
                "Traceroute probe selected fallback candidate !{:08x} after skipping {} cooling-down node(s) across {} window(s)",
                target,
                selection.cooldown_skipped,
                selection.queried_limits
            );
        }

        let channel = match MeshChannel::new(cfg.mesh_channel) {
            Ok(ch) => ch,
            Err(e) => {
                log::error!(
                    "Invalid traceroute mesh_channel {}: {}",
                    cfg.mesh_channel,
                    e
                );
                return;
            }
        };

        self.queue_message(OutgoingMeshMessage {
            kind: OutgoingKind::Traceroute {
                target_node: target,
            },
            text: String::new(),
            destination: PacketDestination::Node(NodeId::from(target)),
            channel,
            from_node: my_node_id,
            to_node: Some(target),
            mesh_channel: cfg.mesh_channel,
            reply_id: None,
        });

        self.traceroute.mark_sent(target);
        log::info!("Queued traceroute probe for !{:08x}", target);
    }
}

fn sanitize_traceroute_jitter_pct(jitter_pct: f64) -> f64 {
    if !jitter_pct.is_finite() || jitter_pct <= 0.0 {
        return 0.0;
    }
    jitter_pct.min(1.0)
}

fn next_traceroute_interval(base: std::time::Duration, jitter_pct: f64) -> std::time::Duration {
    let jitter_pct = sanitize_traceroute_jitter_pct(jitter_pct);
    if jitter_pct == 0.0 {
        return base;
    }

    let jitter_secs = rand::thread_rng().gen_range(0.0..=(base.as_secs_f64() * jitter_pct));
    std::time::Duration::from_secs_f64(base.as_secs_f64() + jitter_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn sanitize_traceroute_jitter_pct_clamps_values() {
        assert_eq!(sanitize_traceroute_jitter_pct(-0.1), 0.0);
        assert_eq!(sanitize_traceroute_jitter_pct(0.0), 0.0);
        assert_eq!(sanitize_traceroute_jitter_pct(0.2), 0.2);
        assert_eq!(sanitize_traceroute_jitter_pct(2.0), 1.0);
        assert_eq!(sanitize_traceroute_jitter_pct(f64::NAN), 0.0);
    }

    #[test]
    fn next_traceroute_interval_respects_bounds() {
        let base = std::time::Duration::from_secs(60);
        for _ in 0..256 {
            let actual = next_traceroute_interval(base, 0.25);
            assert!(actual >= base);
            assert!(actual <= std::time::Duration::from_secs(75));
        }
    }

    #[test]
    fn next_traceroute_interval_zero_jitter_is_fixed() {
        let base = std::time::Duration::from_secs(60);
        assert_eq!(next_traceroute_interval(base, 0.0), base);
    }

    #[test]
    fn select_probe_target_adaptive_expands_and_finds_candidate() {
        let limits = [10usize, 25, 50, 100];
        let mut windows = HashMap::new();
        windows.insert(10usize, (1u32..=10).collect::<Vec<_>>());
        windows.insert(25usize, (1u32..=25).collect::<Vec<_>>());
        let selection = select_probe_target_adaptive(
            &limits,
            |limit| Ok::<Vec<u32>, &'static str>(windows.get(&limit).cloned().unwrap_or_default()),
            |node_id| node_id == 21,
        )
        .unwrap();

        assert_eq!(selection.target, Some(21));
        assert_eq!(selection.cooldown_skipped, 20);
        assert_eq!(selection.queried_limits, 2);
        assert!(selection.had_candidates);
    }

    #[test]
    fn select_probe_target_adaptive_all_cooldown() {
        let limits = [10usize, 25, 50, 100];
        let mut windows = HashMap::new();
        windows.insert(10usize, (1u32..=10).collect::<Vec<_>>());
        windows.insert(25usize, (1u32..=25).collect::<Vec<_>>());
        windows.insert(50usize, (1u32..=30).collect::<Vec<_>>());
        let selection = select_probe_target_adaptive(
            &limits,
            |limit| Ok::<Vec<u32>, &'static str>(windows.get(&limit).cloned().unwrap_or_default()),
            |_node_id| false,
        )
        .unwrap();

        assert_eq!(selection.target, None);
        assert_eq!(selection.cooldown_skipped, 30);
        assert_eq!(selection.queried_limits, 3);
        assert!(selection.had_candidates);
    }

    #[test]
    fn select_probe_target_adaptive_no_candidates() {
        let limits = [10usize, 25, 50, 100];
        let selection = select_probe_target_adaptive(
            &limits,
            |_limit| Ok::<Vec<u32>, &'static str>(Vec::new()),
            |_node_id| true,
        )
        .unwrap();

        assert_eq!(selection.target, None);
        assert_eq!(selection.cooldown_skipped, 0);
        assert_eq!(selection.queried_limits, 1);
        assert!(!selection.had_candidates);
    }
}
