use crate::bridge::{MeshBridgeMessage, OutgoingBridgeMessage};
use crate::message::{MeshEvent, MessageContext};
use meshtastic::packet::PacketDestination;
use meshtastic::protobufs::{self, from_radio, mesh_packet};
use meshtastic::types::MeshChannel;

use super::*;

impl Bot {
    fn empty_routes() -> (Vec<u32>, Vec<u32>) {
        (Vec::new(), Vec::new())
    }

    fn format_route(route: &[u32]) -> String {
        if route.is_empty() {
            return "[]".to_string();
        }
        let nodes = route
            .iter()
            .map(|n| format!("!{:08x}", n))
            .collect::<Vec<_>>()
            .join(" -> ");
        format!("[{}]", nodes)
    }

    fn decode_traceroute_routes(data: &protobufs::Data) -> (Vec<u32>, Vec<u32>) {
        match meshtastic::Message::decode(data.payload.as_slice()) {
            Ok(routing) => {
                let routing: protobufs::Routing = routing;
                match routing.variant {
                    Some(protobufs::routing::Variant::RouteRequest(route)) => {
                        (route.route, route.route_back)
                    }
                    Some(protobufs::routing::Variant::RouteReply(route)) => {
                        if route.route_back.is_empty() {
                            (Vec::new(), route.route)
                        } else {
                            (Vec::new(), route.route_back)
                        }
                    }
                    _ => Self::empty_routes(),
                }
            }
            Err(e) => {
                log::debug!("Failed to decode traceroute payload as Routing: {}", e);
                Self::empty_routes()
            }
        }
    }

    fn decode_routing_variant(data: &protobufs::Data) -> Option<(String, Vec<u32>, Vec<u32>)> {
        match meshtastic::Message::decode(data.payload.as_slice()) {
            Ok(routing) => {
                let routing: protobufs::Routing = routing;
                match routing.variant {
                    Some(protobufs::routing::Variant::RouteRequest(route)) => {
                        let request_len = route.route.len();
                        let response_len = route.route_back.len();
                        Some((
                            format!(
                                "route_request(route_len={}, route_back_len={})",
                                request_len, response_len
                            ),
                            route.route,
                            route.route_back,
                        ))
                    }
                    Some(protobufs::routing::Variant::RouteReply(route)) => {
                        let request_len = route.route.len();
                        let response_len = route.route_back.len();
                        let response_route = if route.route_back.is_empty() {
                            route.route.clone()
                        } else {
                            route.route_back.clone()
                        };
                        Some((
                            format!(
                                "route_reply(route_len={}, route_back_len={})",
                                request_len, response_len
                            ),
                            route.route,
                            response_route,
                        ))
                    }
                    Some(protobufs::routing::Variant::ErrorReason(err)) => {
                        Some((format!("error_reason={:?}", err), Vec::new(), Vec::new()))
                    }
                    None => Some(("none".to_string(), Vec::new(), Vec::new())),
                }
            }
            Err(_) => None,
        }
    }

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
        to_node: Option<u32>,
        rssi: Option<i32>,
        snr: Option<f32>,
        hop_count: Option<u32>,
        hop_start: Option<u32>,
        kind: &str,
    ) -> Option<i64> {
        self.db
            .log_packet_with_mesh_id(
                mesh_packet.from,
                to_node,
                mesh_packet.channel,
                "",
                "in",
                mesh_packet.via_mqtt,
                rssi,
                snr,
                hop_count,
                hop_start,
                Some(mesh_packet.id),
                kind,
            )
            .ok()
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
        let to_node = if mesh_packet.to == 0 {
            None
        } else {
            Some(mesh_packet.to)
        };

        match data.portnum() {
            protobufs::PortNum::PositionApp => {
                self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "position",
                );
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
                self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "telemetry",
                );
            }
            protobufs::PortNum::TracerouteApp => {
                let (request_route, response_route) = Self::decode_traceroute_routes(data);
                let destination = if mesh_packet.to == 0 {
                    "broadcast".to_string()
                } else {
                    format!("!{:08x}", mesh_packet.to)
                };
                let is_response = data.request_id != 0;
                let session_src = if is_response {
                    to_node.unwrap_or(mesh_packet.from)
                } else {
                    mesh_packet.from
                };
                let session_dst = if is_response {
                    Some(mesh_packet.from)
                } else {
                    to_node
                };
                let request_mesh_id = if is_response {
                    data.request_id
                } else {
                    mesh_packet.id
                };
                let trace_key =
                    Self::traceroute_session_key(session_src, session_dst, request_mesh_id);
                log::info!(
                    "Traceroute from !{:08x} to {} [msg_id={}] (ch={}, {}, hops={}/{}, rssi={}, snr={:.1})",
                    mesh_packet.from,
                    destination,
                    mesh_packet.id,
                    mesh_packet.channel,
                    if mesh_packet.via_mqtt { "MQTT" } else { "RF" },
                    hop_count.unwrap_or(0),
                    hop_start.unwrap_or(mesh_packet.hop_start),
                    mesh_packet.rx_rssi,
                    mesh_packet.rx_snr
                );
                log::trace!(
                    "Traceroute detail [msg_id={} trace_key={}]: request_len={} request_path={} response_len={} response_path={}",
                    mesh_packet.id,
                    trace_key,
                    request_route.len(),
                    Self::format_route(&request_route),
                    response_route.len(),
                    Self::format_route(&response_route)
                );
                if let Some(packet_row_id) = self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "traceroute",
                ) {
                    log::trace!(
                        "Traceroute packet logged [msg_id={} trace_key={} packet_row_id={}]",
                        mesh_packet.id,
                        trace_key,
                        packet_row_id
                    );
                    match self.db.log_traceroute_observation(
                        packet_row_id,
                        &trace_key,
                        session_src,
                        session_dst,
                        mesh_packet.via_mqtt,
                        if is_response { None } else { hop_count },
                        if is_response { None } else { hop_start },
                        if is_response { hop_count } else { None },
                        if is_response { hop_start } else { None },
                        if is_response { &[] } else { &request_route },
                        if is_response {
                            if response_route.is_empty() {
                                &request_route
                            } else {
                                &response_route
                            }
                        } else {
                            &response_route
                        },
                        "route",
                        "route_back",
                    ) {
                        Ok(()) => {
                            log::trace!(
                                "Traceroute session updated [msg_id={} trace_key={} packet_row_id={} req_hops={:?} req_start={:?}]",
                                mesh_packet.id,
                                trace_key,
                                packet_row_id,
                                hop_count,
                                hop_start
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "Traceroute session update failed [msg_id={} trace_key={} packet_row_id={}]: {}",
                                mesh_packet.id,
                                trace_key,
                                packet_row_id,
                                e
                            );
                        }
                    }
                } else {
                    log::error!(
                        "Traceroute packet log insert failed [msg_id={} trace_key={}]",
                        mesh_packet.id,
                        trace_key
                    );
                }
            }
            protobufs::PortNum::NeighborinfoApp => {
                self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "neighborinfo",
                );
            }
            protobufs::PortNum::RoutingApp => {
                let (routing_variant, routing_request_route, routing_response_route) =
                    Self::decode_routing_variant(data)
                        .unwrap_or_else(|| ("decode_failed".to_string(), Vec::new(), Vec::new()));
                log::trace!(
                    "Routing packet [msg_id={} from=!{:08x} to={} request_id={} reply_id={} variant={} req_path={} res_path={}]",
                    mesh_packet.id,
                    mesh_packet.from,
                    if mesh_packet.to == 0 {
                        "broadcast".to_string()
                    } else {
                        format!("!{:08x}", mesh_packet.to)
                    },
                    data.request_id,
                    data.reply_id,
                    routing_variant,
                    Self::format_route(&routing_request_route),
                    Self::format_route(&routing_response_route),
                );
                let packet_row_id = self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "routing",
                );
                if data.request_id != 0 {
                    match self
                        .db
                        .find_traceroute_session_by_request_mesh_id(data.request_id, 3600)
                    {
                        Ok(Some((trace_key, session_src, session_dst))) => {
                            log::trace!(
                                "Routing correlation matched traceroute session [routing_msg_id={} request_id={} trace_key={} src=!{:08x} dst={}]",
                                mesh_packet.id,
                                data.request_id,
                                trace_key,
                                session_src,
                                session_dst
                                    .map(|n| format!("!{:08x}", n))
                                    .unwrap_or_else(|| "broadcast".to_string())
                            );
                            if let Some(packet_row_id) = packet_row_id {
                                match self.db.log_traceroute_observation(
                                    packet_row_id,
                                    &trace_key,
                                    session_src,
                                    session_dst,
                                    mesh_packet.via_mqtt,
                                    None,
                                    None,
                                    hop_count,
                                    hop_start,
                                    &routing_request_route,
                                    &routing_response_route,
                                    "routing_route",
                                    "routing_route_back",
                                ) {
                                    Ok(()) => log::trace!(
                                        "Routing-updated traceroute session [routing_msg_id={} trace_key={} packet_row_id={}]",
                                        mesh_packet.id,
                                        trace_key,
                                        packet_row_id
                                    ),
                                    Err(e) => log::error!(
                                        "Routing->traceroute session update failed [routing_msg_id={} trace_key={} packet_row_id={}]: {}",
                                        mesh_packet.id,
                                        trace_key,
                                        packet_row_id,
                                        e
                                    ),
                                }
                            }
                        }
                        Ok(None) => {
                            log::trace!(
                                "Routing correlation skipped (no matching traceroute request session) [routing_msg_id={} request_id={} from=!{:08x} to={}]",
                                mesh_packet.id,
                                data.request_id,
                                mesh_packet.from,
                                if mesh_packet.to == 0 {
                                    "broadcast".to_string()
                                } else {
                                    format!("!{:08x}", mesh_packet.to)
                                }
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "Routing correlation lookup failed [routing_msg_id={} request_id={}]: {}",
                                mesh_packet.id,
                                data.request_id,
                                e
                            );
                        }
                    }
                }
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
                self.log_incoming_packet(
                    mesh_packet,
                    to_node,
                    rssi,
                    snr,
                    hop_count,
                    hop_start,
                    "other",
                );
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
            hop_start: mesh_packet.hop_start,
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
        let _ = self.db.log_packet_with_mesh_id(
            mesh_packet.from,
            if mesh_packet.to == 0 {
                None
            } else {
                Some(mesh_packet.to)
            },
            mesh_packet.channel,
            text,
            "in",
            mesh_packet.via_mqtt,
            rssi,
            snr,
            hop_count,
            hop_start,
            Some(mesh_packet.id),
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
        let _ = self.db.log_packet_with_mesh_id(
            node_id, None, 0, "", "in", via_mqtt, None, None, None, None, None, "nodeinfo",
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

#[cfg(test)]
mod tests {
    use super::*;
    use meshtastic::Message;

    fn routing_data(variant: protobufs::routing::Variant) -> protobufs::Data {
        let routing = protobufs::Routing {
            variant: Some(variant),
        };
        protobufs::Data {
            portnum: protobufs::PortNum::RoutingApp as i32,
            payload: routing.encode_to_vec().into(),
            ..Default::default()
        }
    }

    #[test]
    fn test_decode_routing_variant_route_request_extracts_paths() {
        let data = routing_data(protobufs::routing::Variant::RouteRequest(
            protobufs::RouteDiscovery {
                route: vec![0x11111111, 0x22222222],
                snr_towards: vec![],
                route_back: vec![0x33333333, 0x44444444],
                snr_back: vec![],
            },
        ));

        let (label, req, res) = Bot::decode_routing_variant(&data).expect("decoded");
        assert!(label.starts_with("route_request("));
        assert_eq!(req, vec![0x11111111, 0x22222222]);
        assert_eq!(res, vec![0x33333333, 0x44444444]);
    }

    #[test]
    fn test_decode_routing_variant_route_reply_prefers_route_back_or_route() {
        let with_back = routing_data(protobufs::routing::Variant::RouteReply(
            protobufs::RouteDiscovery {
                route: vec![0xaaaaaaaa],
                snr_towards: vec![],
                route_back: vec![0xbbbbbbbb],
                snr_back: vec![],
            },
        ));
        let (_, req1, res1) = Bot::decode_routing_variant(&with_back).expect("decoded");
        assert_eq!(req1, vec![0xaaaaaaaa]);
        assert_eq!(res1, vec![0xbbbbbbbb]);

        let no_back = routing_data(protobufs::routing::Variant::RouteReply(
            protobufs::RouteDiscovery {
                route: vec![0xcccccccc, 0xdddddddd],
                snr_towards: vec![],
                route_back: vec![],
                snr_back: vec![],
            },
        ));
        let (_, req2, res2) = Bot::decode_routing_variant(&no_back).expect("decoded");
        assert_eq!(req2, vec![0xcccccccc, 0xdddddddd]);
        assert_eq!(res2, vec![0xcccccccc, 0xdddddddd]);
    }

    #[test]
    fn test_decode_routing_variant_returns_none_for_invalid_payload() {
        let data = protobufs::Data {
            portnum: protobufs::PortNum::RoutingApp as i32,
            payload: vec![0xff, 0x00, 0x13].into(),
            ..Default::default()
        };
        assert!(Bot::decode_routing_variant(&data).is_none());
    }
}
