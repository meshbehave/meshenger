use meshtastic::api::StreamApi;
use meshtastic::packet::{PacketDestination, PacketRouter};
use meshtastic::protobufs::{self, from_radio};
use meshtastic::types::{MeshChannel, NodeId};
use meshtastic::utils;
use meshtastic::utils::stream::build_tcp_stream;
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
        let traceroute_interval =
            std::time::Duration::from_secs(self.config.traceroute_probe.interval_secs.max(60));
        let traceroute_timer = tokio::time::sleep(traceroute_interval);
        tokio::pin!(traceroute_timer);
        let stale_node_max_age = std::time::Duration::from_secs(7 * 24 * 60 * 60);
        let stale_node_purge_interval = std::time::Duration::from_secs(60 * 60);
        let stale_node_purge_timer = tokio::time::sleep(stale_node_purge_interval);
        tokio::pin!(stale_node_purge_timer);

        self.purge_stale_nodes(stale_node_max_age);

        // Track bridge receiver availability; disable bridge polling once channel closes.
        let mut bridge_rx = self.bridge.rx();

        loop {
            // Check if the outgoing queue has messages to send
            let queue_has_messages = !self.outgoing.is_empty();

            // If we have a bridge receiver, use select to handle both
            if let Some(rx_mutex) = bridge_rx {
                let mut disable_bridge = false;
                let mut rx_guard = rx_mutex.lock().await;
                tokio::select! {
                    // Handle packets from mesh
                    packet = packet_rx.recv() => {
                        match packet {
                            Some(p) => {
                                drop(rx_guard); // Release lock before processing
                                self.process_radio_packet(my_node_id, p).await;
                            }
                            None => {
                                log::warn!("Packet channel closed, exiting event loop");
                                return Ok(());
                            }
                        }
                    }
                    // Handle messages from bridges
                    bridge_msg = rx_guard.recv() => {
                        match bridge_msg {
                            Some(msg) => {
                                drop(rx_guard); // Release lock before processing
                                self.handle_bridge_message(my_node_id, msg);
                            }
                            None => {
                                drop(rx_guard);
                                disable_bridge = true;
                                log::warn!("Bridge outgoing channel closed; disabling bridge receive path");
                            }
                        }
                    }
                    // Dispatch deferred events after grace period
                    _ = &mut grace_timer, if !grace_period_done => {
                        drop(rx_guard);
                        grace_period_done = true;
                        self.dispatch_deferred_events(my_node_id).await;
                    }
                    // Drain outgoing queue
                    _ = &mut send_timer, if queue_has_messages => {
                        drop(rx_guard);
                        self.send_next_queued_message(&mut api, router).await;
                        self.notify_dashboard();
                        send_timer.as_mut().reset(tokio::time::Instant::now() + send_delay);
                    }
                    _ = &mut traceroute_timer, if traceroute_enabled => {
                        drop(rx_guard);
                        self.maybe_queue_traceroute_probe(my_node_id);
                        traceroute_timer.as_mut().reset(tokio::time::Instant::now() + traceroute_interval);
                    }
                    _ = &mut stale_node_purge_timer => {
                        drop(rx_guard);
                        self.purge_stale_nodes(stale_node_max_age);
                        stale_node_purge_timer.as_mut().reset(tokio::time::Instant::now() + stale_node_purge_interval);
                    }
                }
                if disable_bridge {
                    bridge_rx = None;
                }
            } else {
                // No bridges, just handle mesh packets
                tokio::select! {
                    packet = packet_rx.recv() => {
                        match packet {
                            Some(packet) => {
                                self.process_radio_packet(my_node_id, packet).await;
                            }
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
                    // Drain outgoing queue
                    _ = &mut send_timer, if queue_has_messages => {
                        self.send_next_queued_message(&mut api, router).await;
                        self.notify_dashboard();
                        send_timer.as_mut().reset(tokio::time::Instant::now() + send_delay);
                    }
                    _ = &mut traceroute_timer, if traceroute_enabled => {
                        self.maybe_queue_traceroute_probe(my_node_id);
                        traceroute_timer.as_mut().reset(tokio::time::Instant::now() + traceroute_interval);
                    }
                    _ = &mut stale_node_purge_timer => {
                        self.purge_stale_nodes(stale_node_max_age);
                        stale_node_purge_timer.as_mut().reset(tokio::time::Instant::now() + stale_node_purge_interval);
                    }
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
            return;
        }

        let target = match self
            .db
            .recent_rf_node_missing_hops(cfg.recent_seen_within_secs, Some(my_node_id))
        {
            Ok(Some(node_id)) => node_id,
            Ok(None) => return,
            Err(e) => {
                log::error!("Traceroute probe candidate query failed: {}", e);
                return;
            }
        };

        if !self.traceroute.can_send(target, cfg.per_node_cooldown_secs) {
            return;
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
