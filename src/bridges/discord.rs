//! Discord bridge for Meshenger.
//!
//! Bridges messages between a Discord channel and the Meshtastic mesh.

use std::sync::Arc;

use serenity::all::{
    ChannelId, Context, CreateMessage, EventHandler, GatewayIntents, Message, Ready,
};
use serenity::async_trait;
use serenity::Client;
use tokio::sync::RwLock;

use crate::bridge::{
    MeshBridgeMessage, MeshMessageReceiver, OutgoingBridgeMessage, OutgoingMessageSender,
};

/// Direction of message bridging.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeDirection {
    /// Only forward mesh messages to Discord
    ToDiscord,
    /// Only forward Discord messages to mesh
    ToMesh,
    /// Bidirectional bridging
    Both,
}

impl BridgeDirection {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "to_discord" | "todiscord" | "mesh_to_discord" => BridgeDirection::ToDiscord,
            "to_mesh" | "tomesh" | "discord_to_mesh" => BridgeDirection::ToMesh,
            _ => BridgeDirection::Both,
        }
    }

    pub fn forwards_to_discord(&self) -> bool {
        matches!(self, BridgeDirection::ToDiscord | BridgeDirection::Both)
    }

    pub fn forwards_to_mesh(&self) -> bool {
        matches!(self, BridgeDirection::ToMesh | BridgeDirection::Both)
    }
}

/// Configuration for the Discord bridge.
#[derive(Debug, Clone)]
pub struct DiscordBridgeConfig {
    pub bot_token: String,
    pub channel_id: u64,
    pub mesh_channel: u32,
    pub direction: BridgeDirection,
    pub format: String,
}

impl Default for DiscordBridgeConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            channel_id: 0,
            mesh_channel: 0,
            direction: BridgeDirection::Both,
            format: "**{name}**: {message}".to_string(),
        }
    }
}

/// Shared state for the Discord event handler.
struct HandlerState {
    config: DiscordBridgeConfig,
    outgoing_tx: OutgoingMessageSender,
}

/// Discord event handler.
struct Handler {
    state: Arc<RwLock<HandlerState>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _ctx: Context, msg: Message) {
        // Ignore messages from bots (including ourselves)
        if msg.author.bot {
            return;
        }

        let state = self.state.read().await;

        // Only process messages from the configured channel
        if msg.channel_id.get() != state.config.channel_id {
            return;
        }

        // Skip if not forwarding to mesh
        if !state.config.direction.forwards_to_mesh() {
            return;
        }

        let content = msg.content.trim();
        if content.is_empty() {
            return;
        }

        // Get sender name
        let sender_name = msg.author.name.clone();

        // Format message for mesh
        let mesh_text = format!("[DC:{}] {}", sender_name, content);

        // Check message length (Meshtastic limit ~230 bytes)
        let mesh_text = if mesh_text.len() > 220 {
            format!("{}...", &mesh_text[..217])
        } else {
            mesh_text
        };

        log::debug!("Forwarding to mesh: {}", mesh_text);

        // Send to mesh
        if let Err(e) = state
            .outgoing_tx
            .send(OutgoingBridgeMessage {
                text: mesh_text,
                channel: state.config.mesh_channel,
                source: "discord".to_string(),
            })
            .await
        {
            log::error!("Failed to send to mesh: {}", e);
        }
    }

    async fn ready(&self, _ctx: Context, ready: Ready) {
        log::info!("Discord bot connected as {}", ready.user.name);
    }
}

/// Discord bridge instance.
pub struct DiscordBridge {
    config: DiscordBridgeConfig,
}

impl DiscordBridge {
    /// Create a new Discord bridge with the given configuration.
    pub fn new(config: DiscordBridgeConfig) -> Self {
        Self { config }
    }

    /// Format a mesh message for Discord.
    fn format_mesh_message(config: &DiscordBridgeConfig, msg: &MeshBridgeMessage) -> String {
        config
            .format
            .replace("{name}", &msg.sender_name)
            .replace("{id}", &format!("!{:08x}", msg.sender_id))
            .replace("{message}", &msg.text)
            .replace("{channel}", &msg.channel.to_string())
    }

    /// Run the Discord bridge.
    pub async fn run(
        self,
        mesh_rx: MeshMessageReceiver,
        outgoing_tx: OutgoingMessageSender,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "Starting Discord bridge (channel_id={}, direction={:?})",
            self.config.channel_id,
            self.config.direction
        );

        let config = self.config.clone();
        let channel_id = ChannelId::new(config.channel_id);

        // Create shared state for the handler
        let state = Arc::new(RwLock::new(HandlerState {
            config: config.clone(),
            outgoing_tx,
        }));

        let handler = Handler {
            state: state.clone(),
        };

        // Set up intents
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT
            | GatewayIntents::DIRECT_MESSAGES;

        // Build the client
        let mut client = Client::builder(&config.bot_token, intents)
            .event_handler(handler)
            .await?;

        // Get HTTP client for sending messages
        let http = client.http.clone();

        // Spawn mesh→discord forwarder
        if config.direction.forwards_to_discord() {
            let config_clone = config.clone();
            let http_clone = http.clone();

            tokio::spawn(async move {
                Self::mesh_to_discord_task(http_clone, channel_id, config_clone, mesh_rx).await;
            });
        }

        // Run the Discord client (this blocks)
        if let Err(e) = client.start().await {
            log::error!("Discord client error: {}", e);
            return Err(e.into());
        }

        Ok(())
    }

    /// Task that forwards mesh messages to Discord.
    async fn mesh_to_discord_task(
        http: Arc<serenity::http::Http>,
        channel_id: ChannelId,
        config: DiscordBridgeConfig,
        mut mesh_rx: MeshMessageReceiver,
    ) {
        log::info!("Mesh→Discord forwarder started");

        loop {
            match mesh_rx.recv().await {
                Ok(msg) => {
                    // Only forward messages from the configured mesh channel
                    // Channel 0 means "all channels"
                    if config.mesh_channel != 0 && msg.channel != config.mesh_channel {
                        continue;
                    }

                    // Skip DMs (only bridge public messages)
                    if msg.is_dm {
                        continue;
                    }

                    let text = Self::format_mesh_message(&config, &msg);

                    log::debug!("Forwarding to Discord: {}", text);

                    let builder = CreateMessage::new().content(&text);
                    if let Err(e) = channel_id.send_message(&http, builder).await {
                        log::error!("Failed to send to Discord: {}", e);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("Discord bridge lagged, missed {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("Mesh channel closed, stopping Discord forwarder");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_direction_from_str() {
        assert_eq!(
            BridgeDirection::from_str("to_discord"),
            BridgeDirection::ToDiscord
        );
        assert_eq!(
            BridgeDirection::from_str("mesh_to_discord"),
            BridgeDirection::ToDiscord
        );
        assert_eq!(
            BridgeDirection::from_str("to_mesh"),
            BridgeDirection::ToMesh
        );
        assert_eq!(
            BridgeDirection::from_str("discord_to_mesh"),
            BridgeDirection::ToMesh
        );
        assert_eq!(BridgeDirection::from_str("both"), BridgeDirection::Both);
        assert_eq!(BridgeDirection::from_str("unknown"), BridgeDirection::Both);
    }

    #[test]
    fn test_bridge_direction_forwards() {
        assert!(BridgeDirection::ToDiscord.forwards_to_discord());
        assert!(!BridgeDirection::ToDiscord.forwards_to_mesh());

        assert!(!BridgeDirection::ToMesh.forwards_to_discord());
        assert!(BridgeDirection::ToMesh.forwards_to_mesh());

        assert!(BridgeDirection::Both.forwards_to_discord());
        assert!(BridgeDirection::Both.forwards_to_mesh());
    }

    #[test]
    fn test_format_mesh_message() {
        let config = DiscordBridgeConfig {
            format: "**{name}**: {message}".to_string(),
            ..Default::default()
        };

        let msg = MeshBridgeMessage {
            sender_id: 0xaabbccdd,
            sender_name: "Alice".to_string(),
            text: "Hello world".to_string(),
            channel: 0,
            is_dm: false,
        };

        assert_eq!(
            DiscordBridge::format_mesh_message(&config, &msg),
            "**Alice**: Hello world"
        );
    }

    #[test]
    fn test_format_mesh_message_with_id() {
        let config = DiscordBridgeConfig {
            format: "`{id}` **{name}**: {message}".to_string(),
            ..Default::default()
        };

        let msg = MeshBridgeMessage {
            sender_id: 0x12345678,
            sender_name: "Bob".to_string(),
            text: "Test".to_string(),
            channel: 0,
            is_dm: false,
        };

        assert_eq!(
            DiscordBridge::format_mesh_message(&config, &msg),
            "`!12345678` **Bob**: Test"
        );
    }
}
