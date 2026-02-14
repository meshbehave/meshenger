use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use meshtastic::api::StreamApi;
use meshtastic::packet::{PacketDestination, PacketRouter};
use meshtastic::protobufs::{self, from_radio, mesh_packet};
use meshtastic::types::{MeshChannel, NodeId};
use meshtastic::utils;
use meshtastic::utils::stream::build_tcp_stream;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::bridge::{MeshBridgeMessage, MeshMessageSender, OutgoingBridgeMessage, OutgoingMessageReceiver};
use crate::config::Config;
use crate::db::Db;
use crate::message::{Destination, MeshEvent, MessageContext, Response};
use crate::module::ModuleRegistry;

#[derive(Debug, Clone)]
struct OutgoingMeshMessage {
    text: String,
    destination: PacketDestination,
    channel: MeshChannel,
    /// Bot's own node ID (for DB logging as sender)
    from_node: u32,
    /// Target node ID for DB logging (None = broadcast)
    to_node: Option<u32>,
    /// Meshtastic channel index for DB logging
    mesh_channel: u32,
}

const MAX_MESSAGE_LEN: usize = 220;

#[derive(Debug)]
struct RouterError(String);

impl std::fmt::Display for RouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RouterError {}

struct BotPacketRouter {
    node_id: u32,
}

impl PacketRouter<(), RouterError> for BotPacketRouter {
    fn handle_packet_from_radio(&mut self, _packet: protobufs::FromRadio) -> Result<(), RouterError> {
        Ok(())
    }

    fn handle_mesh_packet(&mut self, _packet: protobufs::MeshPacket) -> Result<(), RouterError> {
        Ok(())
    }

    fn source_node_id(&self) -> NodeId {
        NodeId::from(self.node_id)
    }
}

struct RateLimiter {
    commands: Mutex<HashMap<u32, Vec<Instant>>>,
    max_commands: usize,
    window_secs: u64,
}

impl RateLimiter {
    fn new(max_commands: usize, window_secs: u64) -> Self {
        Self {
            commands: Mutex::new(HashMap::new()),
            max_commands,
            window_secs,
        }
    }

    fn check(&self, node_id: u32) -> bool {
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

/// Duration after connect during which NodeInfo events are deferred.
/// The Meshtastic node dumps all known nodes on connect — dispatching events
/// immediately would spam greetings. Deferred events are dispatched once after
/// this period ends.
const STARTUP_GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(30);

pub struct Bot {
    config: Arc<Config>,
    db: Arc<Db>,
    registry: Arc<ModuleRegistry>,
    rate_limiter: RateLimiter,
    /// Tracks when the current connection started (for startup grace period)
    connected_at: Mutex<Option<Instant>>,
    /// NodeInfo events deferred during startup grace period
    deferred_events: Mutex<Vec<MeshEvent>>,
    /// Channel to broadcast mesh messages to bridges
    bridge_tx: Option<MeshMessageSender>,
    /// Channel to receive messages from bridges
    bridge_rx: Option<tokio::sync::Mutex<OutgoingMessageReceiver>>,
    /// Outgoing message queue drained by the event loop timer
    outgoing_queue: Mutex<VecDeque<OutgoingMeshMessage>>,
}

impl Bot {
    pub fn new(config: Config, db: Db, registry: ModuleRegistry) -> Self {
        let rate_limiter = RateLimiter::new(
            config.bot.rate_limit_commands,
            config.bot.rate_limit_window_secs,
        );
        Self {
            config: Arc::new(config),
            db: Arc::new(db),
            registry: Arc::new(registry),
            rate_limiter,
            connected_at: Mutex::new(None),
            deferred_events: Mutex::new(Vec::new()),
            bridge_tx: None,
            bridge_rx: None,
            outgoing_queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Set bridge channels for communication with external platforms.
    pub fn with_bridge_channels(
        mut self,
        bridge_tx: MeshMessageSender,
        bridge_rx: OutgoingMessageReceiver,
    ) -> Self {
        self.bridge_tx = Some(bridge_tx);
        self.bridge_rx = Some(tokio::sync::Mutex::new(bridge_rx));
        self
    }

    fn queue_message(&self, msg: OutgoingMeshMessage) {
        self.outgoing_queue.lock().unwrap().push_back(msg);
    }

    fn queue_responses(&self, ctx: &MessageContext, responses: &[Response], my_node_id: u32) {
        for response in responses {
            let destination = match &response.destination {
                Destination::Sender => PacketDestination::Node(NodeId::from(ctx.sender_id)),
                Destination::Broadcast => PacketDestination::Broadcast,
                Destination::Node(id) => PacketDestination::Node(NodeId::from(*id)),
            };

            let channel = match MeshChannel::new(response.channel) {
                Ok(ch) => ch,
                Err(e) => {
                    log::error!("Invalid channel {}: {}", response.channel, e);
                    continue;
                }
            };

            let to_node = match &response.destination {
                Destination::Sender => Some(ctx.sender_id),
                Destination::Node(id) => Some(*id),
                Destination::Broadcast => None,
            };

            let chunks = chunk_message(&response.text);
            for chunk in chunks {
                self.queue_message(OutgoingMeshMessage {
                    text: chunk,
                    destination,
                    channel,
                    from_node: my_node_id,
                    to_node,
                    mesh_channel: response.channel,
                });
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let reconnect_delay = std::time::Duration::from_secs(
            self.config.connection.reconnect_delay_secs,
        );

        loop {
            match self.connect_and_run().await {
                Ok(()) => {
                    log::warn!("Connection closed cleanly");
                }
                Err(e) => {
                    log::error!("Connection error: {}", e);
                }
            }

            log::info!(
                "Reconnecting in {} seconds...",
                reconnect_delay.as_secs()
            );
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

        let mut router = BotPacketRouter { node_id: my_node_id };

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
        *self.connected_at.lock().unwrap() = Some(Instant::now());
        self.deferred_events.lock().unwrap().clear();

        // Timer to dispatch deferred events after the grace period
        let grace_timer = tokio::time::sleep(STARTUP_GRACE_PERIOD);
        tokio::pin!(grace_timer);
        let mut grace_period_done = false;

        // Timer for draining the outgoing message queue
        let send_delay = std::time::Duration::from_millis(self.config.bot.send_delay_ms);
        let send_timer = tokio::time::sleep(send_delay);
        tokio::pin!(send_timer);

        // Take ownership of bridge_rx if present
        let bridge_rx = match &self.bridge_rx {
            Some(rx) => Some(rx),
            None => None,
        };

        loop {
            // Check if the outgoing queue has messages to send
            let queue_has_messages = !self.outgoing_queue.lock().unwrap().is_empty();

            // If we have a bridge receiver, use select to handle both
            if let Some(rx_mutex) = &bridge_rx {
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
                        if let Some(msg) = bridge_msg {
                            drop(rx_guard); // Release lock before processing
                            self.handle_bridge_message(my_node_id, msg);
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
                        send_timer.as_mut().reset(tokio::time::Instant::now() + send_delay);
                    }
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
                        send_timer.as_mut().reset(tokio::time::Instant::now() + send_delay);
                    }
                }
            }
        }
    }

    /// Pop and send the next message from the outgoing queue.
    async fn send_next_queued_message(
        &self,
        api: &mut meshtastic::api::ConnectedStreamApi,
        router: &mut BotPacketRouter,
    ) {
        let msg = match self.outgoing_queue.lock().unwrap().pop_front() {
            Some(m) => m,
            None => return,
        };

        log::info!("Sending queued: {:?} -> {:?}", msg.text, msg.destination);

        // Log outgoing message
        let _ = self.db.log_message(
            msg.from_node,
            msg.to_node,
            msg.mesh_channel,
            &msg.text,
            "out",
        );

        if let Err(e) = api
            .send_text(
                router,
                msg.text,
                msg.destination,
                true,
                msg.channel,
            )
            .await
        {
            log::error!("Failed to send queued message: {}", e);
        }
    }

    async fn process_radio_packet(
        &self,
        my_node_id: u32,
        packet: protobufs::FromRadio,
    ) {
        let variant = match packet.payload_variant {
            Some(v) => v,
            None => return,
        };

        match variant {
            from_radio::PayloadVariant::Packet(mesh_packet) => {
                self.handle_mesh_packet(my_node_id, &mesh_packet).await;
            }
            from_radio::PayloadVariant::NodeInfo(node_info) => {
                self.handle_node_info(my_node_id, &node_info).await;
            }
            _ => {}
        }
    }

    /// Handle a message from an external bridge (Telegram, Discord, etc.)
    fn handle_bridge_message(
        &self,
        my_node_id: u32,
        msg: OutgoingBridgeMessage,
    ) {
        log::info!("Bridge message from {}: {}", msg.source, msg.text);

        let channel = match MeshChannel::new(msg.channel) {
            Ok(ch) => ch,
            Err(e) => {
                log::error!("Invalid channel {}: {}", msg.channel, e);
                return;
            }
        };

        self.queue_message(OutgoingMeshMessage {
            text: msg.text,
            destination: PacketDestination::Broadcast,
            channel,
            from_node: my_node_id,
            to_node: None,
            mesh_channel: msg.channel,
        });
    }

    async fn handle_mesh_packet(
        &self,
        my_node_id: u32,
        mesh_packet: &protobufs::MeshPacket,
    ) {
        let data = match &mesh_packet.payload_variant {
            Some(mesh_packet::PayloadVariant::Decoded(data)) => data,
            _ => return,
        };

        // Handle position packets
        if data.portnum() == protobufs::PortNum::PositionApp {
            if let Ok(pos) = meshtastic::Message::decode(data.payload.as_slice()) {
                let pos: protobufs::Position = pos;
                if let (Some(lat_i), Some(lon_i)) = (pos.latitude_i, pos.longitude_i) {
                    let lat = lat_i as f64 * 1e-7;
                    let lon = lon_i as f64 * 1e-7;
                    if lat != 0.0 || lon != 0.0 {
                        log::debug!("Position from !{:08x}: {:.4}, {:.4}", mesh_packet.from, lat, lon);
                        let _ = self.db.update_position(mesh_packet.from, lat, lon);
                    }
                }
            }
            return;
        }

        if data.portnum() != protobufs::PortNum::TextMessageApp {
            return;
        }

        let text = match String::from_utf8(data.payload.clone()) {
            Ok(t) => t,
            Err(_) => return,
        };

        let is_dm = mesh_packet.to == my_node_id;
        let hop_count = mesh_packet.hop_start.saturating_sub(mesh_packet.hop_limit);

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
            hop_count,
            hop_limit: mesh_packet.hop_limit,
            via_mqtt: mesh_packet.via_mqtt,
        };

        log::info!(
            "Text from {} ({}): {}",
            ctx.sender_name,
            if is_dm { "DM" } else { "public" },
            text.trim()
        );

        // Log incoming message
        let _ = self.db.log_message(
            mesh_packet.from,
            if is_dm { Some(my_node_id) } else { None },
            mesh_packet.channel,
            &text,
            "in",
        );

        // Broadcast to bridges (only public messages, skip messages that look like they came from a bridge)
        if !is_dm && !text.starts_with("[TG:") && !text.starts_with("[DC:") {
            if let Some(tx) = &self.bridge_tx {
                let bridge_msg = MeshBridgeMessage {
                    sender_id: mesh_packet.from,
                    sender_name: ctx.sender_name.clone(),
                    text: text.trim().to_string(),
                    channel: mesh_packet.channel,
                    is_dm,
                };
                // Don't block on send, just log if it fails
                if tx.send(bridge_msg).is_err() {
                    log::debug!("No bridge receivers listening");
                }
            }
        }

        // Parse command
        let prefix = &self.config.bot.command_prefix;
        let trimmed = text.trim();
        let (raw_command, args) = match trimmed.split_once(' ') {
            Some((cmd, rest)) => (cmd, rest.trim()),
            None => (trimmed, ""),
        };

        let command = match raw_command.strip_prefix(prefix.as_str()) {
            Some(cmd) => cmd,
            None => return,
        };

        // Rate limit check
        if !self.rate_limiter.check(ctx.sender_id) {
            log::warn!("Rate limited: {} ({})", ctx.sender_name, ctx.sender_id);
            return;
        }

        // Special handling for help: generate text from registry
        if command == "help" {
            let help_text = self.generate_help_text();
            let responses = vec![Response {
                text: help_text,
                destination: Destination::Sender,
                channel: ctx.channel,
            }];
            self.queue_responses(&ctx, &responses, my_node_id);
            return;
        }

        let module = match self.registry.find_by_command(command) {
            Some(m) => m,
            None => return,
        };

        if !module.scope().allows(is_dm) {
            return;
        }

        match module.handle_command(command, args, &ctx, &self.db).await {
            Ok(Some(responses)) => {
                self.queue_responses(&ctx, &responses, my_node_id);
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("Module {} error: {}", module.name(), e);
            }
        }
    }

    async fn handle_node_info(
        &self,
        my_node_id: u32,
        node_info: &protobufs::NodeInfo,
    ) {
        let node_id = node_info.num;
        let (long_name, short_name) = match &node_info.user {
            Some(user) => (user.long_name.clone(), user.short_name.clone()),
            None => (String::new(), String::new()),
        };

        log::debug!(
            "NodeInfo: !{:08x} {} ({})",
            node_id,
            long_name,
            short_name
        );

        // Skip dispatching events for our own node
        if node_id == my_node_id {
            log::debug!("Skipping event dispatch for own node");
            // Still upsert and update position below
        } else {
            // Skip event dispatch during startup grace period (the Meshtastic node
            // dumps all known nodes on connect — greeting them all would be spam)
            let in_grace_period = self
                .connected_at
                .lock()
                .unwrap()
                .map(|t| t.elapsed() < STARTUP_GRACE_PERIOD)
                .unwrap_or(false);

            if in_grace_period {
                log::debug!(
                    "Deferring event dispatch for !{:08x} (startup grace period)",
                    node_id
                );
                self.deferred_events.lock().unwrap().push(MeshEvent::NodeDiscovered {
                    node_id,
                    long_name: long_name.clone(),
                    short_name: short_name.clone(),
                });
                // Skip upsert/position during grace period so nodes stay "new"
                // until deferred events are dispatched
                return;
            } else {
                let event = MeshEvent::NodeDiscovered {
                    node_id,
                    long_name: long_name.clone(),
                    short_name: short_name.clone(),
                };

                // Dispatch event to all modules, queuing any responses
                self.dispatch_event_to_modules(&event, my_node_id).await;
            }
        }

        // Always upsert the node (welcome module may have already done this,
        // but upsert is idempotent and updates last_seen)
        if let Err(e) = self.db.upsert_node(node_id, &short_name, &long_name) {
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

    async fn dispatch_deferred_events(
        &self,
        my_node_id: u32,
    ) {
        let events: Vec<MeshEvent> = {
            let mut deferred = self.deferred_events.lock().unwrap();
            std::mem::take(&mut *deferred)
        };

        if events.is_empty() {
            return;
        }

        log::info!(
            "Grace period ended, dispatching {} deferred event(s)",
            events.len()
        );

        for event in &events {
            if let MeshEvent::NodeDiscovered {
                node_id,
                long_name,
                short_name,
            } = event
            {
                self.dispatch_event_to_modules(event, my_node_id).await;

                // Upsert after module dispatch (was deferred along with the event)
                if let Err(e) = self.db.upsert_node(*node_id, short_name, long_name) {
                    log::error!("Failed to upsert deferred node: {}", e);
                }
            }
        }
    }

    /// Dispatch an event to all modules, queuing any responses.
    async fn dispatch_event_to_modules(&self, event: &MeshEvent, my_node_id: u32) {
        let (node_id, long_name) = match event {
            MeshEvent::NodeDiscovered { node_id, long_name, .. } => (*node_id, long_name.clone()),
            MeshEvent::PositionUpdate { node_id, .. } => (*node_id, String::new()),
        };

        for module in self.registry.all() {
            match module.handle_event(event, &self.db).await {
                Ok(Some(responses)) => {
                    let ctx = MessageContext {
                        sender_id: node_id,
                        sender_name: if !long_name.is_empty() {
                            long_name.clone()
                        } else {
                            format!("!{:08x}", node_id)
                        },
                        channel: 0,
                        is_dm: false,
                        rssi: 0,
                        snr: 0.0,
                        hop_count: 0,
                        hop_limit: 0,
                        via_mqtt: false,
                    };
                    self.queue_responses(&ctx, &responses, my_node_id);
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!("Module {} event error: {}", module.name(), e);
                }
            }
        }
    }

    fn generate_help_text(&self) -> String {
        let prefix = &self.config.bot.command_prefix;
        let mut lines = Vec::new();
        for module in self.registry.all() {
            let cmds = module.commands();
            if !cmds.is_empty() {
                let cmd_str = cmds
                    .iter()
                    .map(|c| format!("{}{}", prefix, c))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("{} - {}", cmd_str, module.description()));
            }
        }
        if lines.is_empty() {
            "No commands available.".to_string()
        } else {
            lines.join("\n")
        }
    }

}

fn chunk_message(text: &str) -> Vec<String> {
    if text.len() <= MAX_MESSAGE_LEN {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        // If adding this line would exceed limit, flush current chunk
        if !current.is_empty() && current.len() + 1 + line.len() > MAX_MESSAGE_LEN {
            chunks.push(current.clone());
            current.clear();
        }

        // If a single line exceeds the limit, split it by characters
        if line.len() > MAX_MESSAGE_LEN {
            if !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            let mut remaining = line;
            while remaining.len() > MAX_MESSAGE_LEN {
                chunks.push(remaining[..MAX_MESSAGE_LEN].to_string());
                remaining = &remaining[MAX_MESSAGE_LEN..];
            }
            if !remaining.is_empty() {
                current = remaining.to_string();
            }
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use crate::module::ModuleRegistry;
    use std::collections::HashMap;
    use std::path::Path;

    fn test_config() -> Config {
        Config {
            connection: ConnectionConfig {
                address: "127.0.0.1:4403".to_string(),
                reconnect_delay_secs: 5,
            },
            bot: BotConfig {
                name: "TestBot".to_string(),
                db_path: ":memory:".to_string(),
                command_prefix: "!".to_string(),
                rate_limit_commands: 0,
                rate_limit_window_secs: 60,
                send_delay_ms: 1500,
            },
            welcome: WelcomeConfig {
                enabled: false,
                message: String::new(),
                welcome_back_message: String::new(),
                absence_threshold_hours: 48,
                whitelist: Vec::new(),
            },
            weather: WeatherConfig {
                latitude: 0.0,
                longitude: 0.0,
                units: "metric".to_string(),
            },
            modules: HashMap::new(),
            bridge: BridgeConfig::default(),
        }
    }

    fn test_bot() -> Bot {
        let config = test_config();
        let db = Db::open(Path::new(":memory:")).unwrap();
        let registry = ModuleRegistry::new();
        Bot::new(config, db, registry)
    }

    fn test_ctx(sender_id: u32, channel: u32) -> MessageContext {
        MessageContext {
            sender_id,
            sender_name: format!("!{:08x}", sender_id),
            channel,
            is_dm: false,
            rssi: 0,
            snr: 0.0,
            hop_count: 0,
            hop_limit: 0,
            via_mqtt: false,
        }
    }

    #[test]
    fn test_queue_message_ordering() {
        let bot = test_bot();
        let my_node_id = 1;

        for i in 0..5 {
            bot.queue_message(OutgoingMeshMessage {
                text: format!("msg{}", i),
                destination: PacketDestination::Broadcast,
                channel: MeshChannel::new(0).unwrap(),
                from_node: my_node_id,
                to_node: None,
                mesh_channel: 0,
            });
        }

        let mut queue = bot.outgoing_queue.lock().unwrap();
        assert_eq!(queue.len(), 5);
        for i in 0..5 {
            let msg = queue.pop_front().unwrap();
            assert_eq!(msg.text, format!("msg{}", i));
        }
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_responses_chunking() {
        let bot = test_bot();
        let ctx = test_ctx(0xAABBCCDD, 0);
        let my_node_id = 1;

        // Create a response longer than MAX_MESSAGE_LEN (220)
        let long_text = "a".repeat(500);
        let responses = vec![Response {
            text: long_text.clone(),
            destination: Destination::Sender,
            channel: 0,
        }];

        bot.queue_responses(&ctx, &responses, my_node_id);

        let queue = bot.outgoing_queue.lock().unwrap();
        assert!(queue.len() > 1, "Long message should be chunked into multiple queue entries");

        // Verify all chunks are within the limit
        for msg in queue.iter() {
            assert!(msg.text.len() <= MAX_MESSAGE_LEN);
        }

        // Verify total content is preserved
        let reassembled: String = queue.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(reassembled, long_text);
    }

    #[test]
    fn test_queue_responses_preserves_destination() {
        let bot = test_bot();
        let ctx = test_ctx(0x12345678, 3);
        let my_node_id = 1;

        let responses = vec![
            Response {
                text: "to sender".to_string(),
                destination: Destination::Sender,
                channel: 3,
            },
            Response {
                text: "broadcast".to_string(),
                destination: Destination::Broadcast,
                channel: 0,
            },
            Response {
                text: "to node".to_string(),
                destination: Destination::Node(0xDEADBEEF),
                channel: 1,
            },
        ];

        bot.queue_responses(&ctx, &responses, my_node_id);

        let queue = bot.outgoing_queue.lock().unwrap();
        assert_eq!(queue.len(), 3);

        // Sender -> Node(sender_id)
        assert!(matches!(queue[0].destination, PacketDestination::Node(_)));
        assert_eq!(queue[0].to_node, Some(0x12345678));
        assert_eq!(queue[0].mesh_channel, 3);

        // Broadcast
        assert!(matches!(queue[1].destination, PacketDestination::Broadcast));
        assert_eq!(queue[1].to_node, None);
        assert_eq!(queue[1].mesh_channel, 0);

        // Node(specific)
        assert!(matches!(queue[2].destination, PacketDestination::Node(_)));
        assert_eq!(queue[2].to_node, Some(0xDEADBEEF));
        assert_eq!(queue[2].mesh_channel, 1);
    }

    #[test]
    fn test_queue_message_from_bridge() {
        let bot = test_bot();
        let my_node_id = 1;

        let msg = OutgoingBridgeMessage {
            text: "[TG:alice] Hello mesh!".to_string(),
            channel: 2,
            source: "telegram".to_string(),
        };

        bot.handle_bridge_message(my_node_id, msg);

        let queue = bot.outgoing_queue.lock().unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].text, "[TG:alice] Hello mesh!");
        assert!(matches!(queue[0].destination, PacketDestination::Broadcast));
        assert_eq!(queue[0].mesh_channel, 2);
        assert_eq!(queue[0].from_node, my_node_id);
        assert_eq!(queue[0].to_node, None);
    }

    #[test]
    fn test_queue_empty_response_not_enqueued() {
        let bot = test_bot();
        let ctx = test_ctx(0x12345678, 0);
        let my_node_id = 1;

        // Empty response list
        bot.queue_responses(&ctx, &[], my_node_id);

        let queue = bot.outgoing_queue.lock().unwrap();
        assert!(queue.is_empty());
    }
}
