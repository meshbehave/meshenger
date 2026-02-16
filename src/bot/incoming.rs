use crate::bridge::{MeshBridgeMessage, OutgoingBridgeMessage};
use crate::message::{MeshEvent, MessageContext};
use meshtastic::packet::PacketDestination;
use meshtastic::protobufs::{self, from_radio, mesh_packet};
use meshtastic::types::MeshChannel;

use super::*;

impl Bot {
    pub(super) async fn process_radio_packet(&self, my_node_id: u32, packet: protobufs::FromRadio) {
        let variant = match packet.payload_variant {
            Some(v) => v,
            None => return,
        };

        match variant {
            from_radio::PayloadVariant::Packet(mesh_packet) => {
                self.handle_mesh_packet(my_node_id, &mesh_packet).await;
                self.notify_dashboard();
            }
            from_radio::PayloadVariant::NodeInfo(node_info) => {
                self.handle_node_info(my_node_id, &node_info).await;
                self.notify_dashboard();
            }
            _ => {}
        }
    }

    /// Handle a message from an external bridge (Telegram, Discord, etc.)
    pub(super) fn handle_bridge_message(&self, my_node_id: u32, msg: OutgoingBridgeMessage) {
        log::info!("Bridge message from {}: {}", msg.source, msg.text);

        let channel = match MeshChannel::new(msg.channel) {
            Ok(ch) => ch,
            Err(e) => {
                log::error!("Invalid channel {}: {}", msg.channel, e);
                return;
            }
        };

        self.queue_message(OutgoingMeshMessage {
            kind: OutgoingKind::Text,
            text: msg.text,
            destination: PacketDestination::Broadcast,
            channel,
            from_node: my_node_id,
            to_node: None,
            mesh_channel: msg.channel,
            reply_id: None,
        });
    }

    /// Extract RF metadata from a mesh packet for logging.
    fn rf_metadata(
        mesh_packet: &protobufs::MeshPacket,
    ) -> (Option<i32>, Option<f32>, Option<u32>, Option<u32>) {
        let rssi = if mesh_packet.rx_rssi != 0 {
            Some(mesh_packet.rx_rssi)
        } else {
            None
        };
        let snr = if mesh_packet.rx_snr != 0.0 {
            Some(mesh_packet.rx_snr)
        } else {
            None
        };
        let hop_count = mesh_packet.hop_start.checked_sub(mesh_packet.hop_limit);
        let hop_start = if mesh_packet.hop_start > 0 {
            Some(mesh_packet.hop_start)
        } else {
            None
        };
        (rssi, snr, hop_count, hop_start)
    }

    fn log_incoming_packet(
        &self,
        mesh_packet: &protobufs::MeshPacket,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
        kind: &str,
    ) {
        let _ = self.db.log_packet(
            mesh_packet.from,
            None,
            mesh_packet.channel,
            "",
            "in",
            mesh_packet.via_mqtt,
            rssi,
            snr,
            hop_count,
            hop_start,
            kind,
        );
    }

    pub(super) async fn handle_mesh_packet(
        &self,
        my_node_id: u32,
        mesh_packet: &protobufs::MeshPacket,
    ) {
        let data = match &mesh_packet.payload_variant {
            Some(mesh_packet::PayloadVariant::Decoded(data)) => data,
            _ => return,
        };

        let (rssi, snr, hop_count, hop_start) = Self::rf_metadata(mesh_packet);

        match data.portnum() {
            protobufs::PortNum::PositionApp => {
                self.log_incoming_packet(mesh_packet, rssi, snr, hop_count, hop_start, "position");
                // Update position in DB
                if let Ok(pos) = meshtastic::Message::decode(data.payload.as_slice()) {
                    let pos: protobufs::Position = pos;
                    if let (Some(lat_i), Some(lon_i)) = (pos.latitude_i, pos.longitude_i) {
                        let lat = lat_i as f64 * 1e-7;
                        let lon = lon_i as f64 * 1e-7;
                        if lat != 0.0 || lon != 0.0 {
                            log::debug!(
                                "Position from !{:08x} [msg_id={}]: {:.4}, {:.4}",
                                mesh_packet.from,
                                mesh_packet.id,
                                lat,
                                lon
                            );
                            let _ = self.db.update_position(mesh_packet.from, lat, lon);
                        }
                    }
                }
            }
            protobufs::PortNum::TelemetryApp => {
                self.log_incoming_packet(mesh_packet, rssi, snr, hop_count, hop_start, "telemetry");
            }
            protobufs::PortNum::TracerouteApp => {
                self.log_incoming_packet(
                    mesh_packet,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "traceroute",
                );
            }
            protobufs::PortNum::NeighborinfoApp => {
                self.log_incoming_packet(
                    mesh_packet,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "neighborinfo",
                );
            }
            protobufs::PortNum::RoutingApp => {
                self.log_incoming_packet(mesh_packet, rssi, snr, hop_count, hop_start, "routing");
            }
            protobufs::PortNum::TextMessageApp => {
                self.handle_text_message(
                    my_node_id,
                    mesh_packet,
                    data,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                )
                .await;
            }
            _ => {
                self.log_incoming_packet(mesh_packet, rssi, snr, hop_count, hop_start, "other");
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_text_message(
        &self,
        my_node_id: u32,
        mesh_packet: &protobufs::MeshPacket,
        data: &protobufs::Data,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
    ) {
        let text = match std::str::from_utf8(&data.payload) {
            Ok(t) => t,
            Err(_) => return,
        };
        let trimmed_text = text.trim();

        let is_dm = mesh_packet.to == my_node_id;
        let hops = hop_count.unwrap_or(0);

        let sender_name = self
            .db
            .get_node_name(mesh_packet.from)
            .unwrap_or_else(|_| format!("!{:08x}", mesh_packet.from));

        let ctx = MessageContext {
            sender_id: mesh_packet.from,
            sender_name,
            channel: mesh_packet.channel,
            is_dm,
            rssi: mesh_packet.rx_rssi,
            snr: mesh_packet.rx_snr,
            hop_count: hops,
            hop_limit: mesh_packet.hop_limit,
            via_mqtt: mesh_packet.via_mqtt,
            packet_id: mesh_packet.id,
        };

        log::info!(
            "Text from {} ({}) [msg_id={}]: {}",
            ctx.sender_name,
            if is_dm { "DM" } else { "public" },
            ctx.packet_id,
            trimmed_text
        );

        // Log incoming text message with RF metadata
        let _ = self.db.log_packet(
            mesh_packet.from,
            if is_dm { Some(my_node_id) } else { None },
            mesh_packet.channel,
            text,
            "in",
            mesh_packet.via_mqtt,
            rssi,
            snr,
            hop_count,
            hop_start,
            "text",
        );

        // Broadcast to bridges (only public messages, skip messages that look like they came from a bridge)
        if !is_dm && !text.starts_with("[TG:") && !text.starts_with("[DC:") {
            if let Some(tx) = self.bridge.tx() {
                let bridge_msg = MeshBridgeMessage {
                    sender_id: mesh_packet.from,
                    sender_name: ctx.sender_name.clone(),
                    text: trimmed_text.to_string(),
                    channel: mesh_packet.channel,
                    is_dm,
                };
                // Don't block on send, just log if it fails
                if tx.send(bridge_msg).is_err() {
                    log::debug!("No bridge receivers listening [msg_id={}]", ctx.packet_id);
                }
            }
        }

        self.dispatch_command_from_text(my_node_id, &ctx, trimmed_text, is_dm)
            .await;
    }

    pub(super) async fn handle_node_info(&self, my_node_id: u32, node_info: &protobufs::NodeInfo) {
        let node_id = node_info.num;
        let (long_name, short_name) = match &node_info.user {
            Some(user) => (user.long_name.clone(), user.short_name.clone()),
            None => (String::new(), String::new()),
        };

        let via_mqtt = node_info.via_mqtt;

        log::debug!("NodeInfo: !{:08x} {} ({})", node_id, long_name, short_name);

        // Log nodeinfo packet (no RF metadata on NodeInfo)
        let _ = self.db.log_packet(
            node_id, None, 0, "", "in", via_mqtt, None, None, None, None, "nodeinfo",
        );

        // Skip dispatching events for our own node
        if node_id == my_node_id {
            log::debug!("Skipping event dispatch for own node");
            // Still upsert and update position below
        } else {
            // Skip event dispatch during startup grace period (the Meshtastic node
            // dumps all known nodes on connect — greeting them all would be spam)
            let in_grace_period = self
                .startup_state
                .in_grace_period(self.config.bot.startup_grace_secs);

            if in_grace_period {
                log::debug!(
                    "Deferring event dispatch for !{:08x} (startup grace period)",
                    node_id
                );
                self.startup_state.defer_event(MeshEvent::NodeDiscovered {
                    node_id,
                    long_name: long_name.clone(),
                    short_name: short_name.clone(),
                    via_mqtt,
                });
                // Skip upsert/position during grace period so nodes stay "new"
                // until deferred events are dispatched
                return;
            } else {
                let event = MeshEvent::NodeDiscovered {
                    node_id,
                    long_name: long_name.clone(),
                    short_name: short_name.clone(),
                    via_mqtt,
                };

                // Dispatch event to all modules, queuing any responses
                self.dispatch_event_to_modules(&event, my_node_id).await;
            }
        }

        // Always upsert the node (welcome module may have already done this,
        // but upsert is idempotent and updates last_seen)
        if let Err(e) = self
            .db
            .upsert_node(node_id, &short_name, &long_name, via_mqtt)
        {
            log::error!("Failed to upsert node: {}", e);
        }

        // Extract position from NodeInfo if available
        if let Some(pos) = &node_info.position {
            if let (Some(lat_i), Some(lon_i)) = (pos.latitude_i, pos.longitude_i) {
                let lat = lat_i as f64 * 1e-7;
                let lon = lon_i as f64 * 1e-7;
                if lat != 0.0 || lon != 0.0 {
                    let _ = self.db.update_position(node_id, lat, lon);
                }
            }
        }
    }
}
