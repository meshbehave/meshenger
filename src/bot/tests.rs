use super::*;
use super::outgoing::chunk_message;
use crate::bridge::OutgoingBridgeMessage;
use crate::config::*;
use crate::message::{Destination, MessageContext, Response};
use crate::module::{Module, ModuleRegistry};
use async_trait::async_trait;
use meshtastic::packet::PacketDestination;
use meshtastic::types::MeshChannel;
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
            max_message_len: 220,
            startup_grace_secs: 30,
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
        traceroute_probe: TracerouteProbeConfig::default(),
        modules: HashMap::new(),
        bridge: BridgeConfig::default(),
        dashboard: DashboardConfig::default(),
    }
}

fn test_bot() -> Bot {
    let config = Arc::new(test_config());
    let db = Arc::new(Db::open(Path::new(":memory:")).unwrap());
    let registry = ModuleRegistry::new();
    Bot::new(config, db, registry)
}

struct TestCommandModule;

#[async_trait]
impl Module for TestCommandModule {
    fn name(&self) -> &str {
        "test_cmd"
    }

    fn description(&self) -> &str {
        "test command module"
    }

    fn commands(&self) -> &[&str] {
        &["echo"]
    }

    fn scope(&self) -> crate::message::CommandScope {
        crate::message::CommandScope::Both
    }

    async fn handle_command(
        &self,
        _command: &str,
        args: &str,
        _ctx: &MessageContext,
        _db: &Db,
    ) -> Result<Option<Vec<Response>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Some(vec![Response {
            text: format!("echo:{args}"),
            destination: Destination::Sender,
            channel: 0,
            reply_id: None,
        }]))
    }
}

fn test_bot_with_module(module: Box<dyn Module>) -> Bot {
    let config = Arc::new(test_config());
    let db = Arc::new(Db::open(Path::new(":memory:")).unwrap());
    let mut registry = ModuleRegistry::new();
    registry.register(module);
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
        packet_id: 0,
    }
}

#[test]
fn test_queue_message_ordering() {
    let bot = test_bot();
    let my_node_id = 1;

    for i in 0..5 {
        bot.queue_message(OutgoingMeshMessage {
            kind: OutgoingKind::Text,
            text: format!("msg{}", i),
            destination: PacketDestination::Broadcast,
            channel: MeshChannel::new(0).unwrap(),
            from_node: my_node_id,
            to_node: None,
            mesh_channel: 0,
            reply_id: None,
        });
    }

    let mut queue = bot.outgoing.snapshot().into_iter().collect::<std::collections::VecDeque<_>>();
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

    // Create a response longer than max_message_len (220)
    let long_text = "a".repeat(500);
    let responses = vec![Response {
        text: long_text.clone(),
        destination: Destination::Sender,
        channel: 0,
        reply_id: None,
    }];

    bot.queue_responses(&ctx, &responses, my_node_id);

    let queue = bot.outgoing.snapshot();
    assert!(queue.len() > 1, "Long message should be chunked into multiple queue entries");

    // Verify all chunks are within the limit
    for msg in &queue {
        assert!(msg.text.len() <= 220);
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
            reply_id: None,
        },
        Response {
            text: "broadcast".to_string(),
            destination: Destination::Broadcast,
            channel: 0,
            reply_id: None,
        },
        Response {
            text: "to node".to_string(),
            destination: Destination::Node(0xDEADBEEF),
            channel: 1,
            reply_id: None,
        },
    ];

    bot.queue_responses(&ctx, &responses, my_node_id);

    let queue = bot.outgoing.snapshot();
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

    let queue = bot.outgoing.snapshot();
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

    let queue = bot.outgoing.snapshot();
    assert!(queue.is_empty());
}

#[test]
fn test_chunk_message_utf8_safe() {
    let text = "éééé";
    let chunks = chunk_message(text, 3);
    assert_eq!(chunks, vec!["é".to_string(), "é".to_string(), "é".to_string(), "é".to_string()]);
}

#[test]
fn test_chunk_message_zero_max_len() {
    let chunks = chunk_message("hello", 0);
    assert!(chunks.is_empty());
}

#[test]
fn test_chunk_message_utf8_reassembles_without_panic() {
    let text = "héllo世界";
    let chunks = chunk_message(text, 5);
    let reassembled: String = chunks.concat();
    assert_eq!(reassembled, text);
}

#[test]
fn test_chunk_message_utf8_char_larger_than_limit_makes_progress() {
    let text = "😀😀";
    let chunks = chunk_message(text, 3);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks.concat(), text);
}

#[tokio::test]
async fn test_dispatch_command_help_enqueues_reply() {
    let bot = test_bot();
    let mut ctx = test_ctx(0x12345678, 0);
    ctx.packet_id = 42;

    bot.dispatch_command_from_text(1, &ctx, "!help", false).await;

    let queue = bot.outgoing.snapshot();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].reply_id, Some(42));
    assert_eq!(queue[0].to_node, Some(ctx.sender_id));
    assert_eq!(queue[0].text, "No commands available.");
}

#[tokio::test]
async fn test_dispatch_command_module_sets_reply_id_when_missing() {
    let bot = test_bot_with_module(Box::new(TestCommandModule));
    let mut ctx = test_ctx(0x11111111, 0);
    ctx.packet_id = 99;

    bot.dispatch_command_from_text(1, &ctx, "!echo hello", false).await;

    let queue = bot.outgoing.snapshot();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].reply_id, Some(99));
    assert_eq!(queue[0].to_node, Some(ctx.sender_id));
    assert_eq!(queue[0].text, "echo:hello");
}

#[tokio::test]
async fn test_dispatch_command_ignores_non_prefixed_text() {
    let bot = test_bot_with_module(Box::new(TestCommandModule));
    let ctx = test_ctx(0x22222222, 0);

    bot.dispatch_command_from_text(1, &ctx, "echo hello", false).await;

    let queue = bot.outgoing.snapshot();
    assert!(queue.is_empty());
}
