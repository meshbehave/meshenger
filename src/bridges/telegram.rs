//! Telegram bridge for Meshenger.
//!
//! Bridges messages between a Telegram group/channel and the Meshtastic mesh.

use std::sync::Arc;

use teloxide::prelude::*;
use teloxide::types::ParseMode;
use tokio::sync::mpsc;

use crate::bridge::{MeshBridgeMessage, MeshMessageReceiver, OutgoingBridgeMessage, OutgoingMessageSender};

/// Direction of message bridging.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeDirection {
    /// Only forward mesh messages to Telegram
    ToTelegram,
    /// Only forward Telegram messages to mesh
    ToMesh,
    /// Bidirectional bridging
    Both,
}

impl BridgeDirection {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "to_telegram" | "totelegram" | "mesh_to_telegram" => BridgeDirection::ToTelegram,
            "to_mesh" | "tomesh" | "telegram_to_mesh" => BridgeDirection::ToMesh,
            _ => BridgeDirection::Both,
        }
    }

    pub fn forwards_to_telegram(&self) -> bool {
        matches!(self, BridgeDirection::ToTelegram | BridgeDirection::Both)
    }

    pub fn forwards_to_mesh(&self) -> bool {
        matches!(self, BridgeDirection::ToMesh | BridgeDirection::Both)
    }
}

/// Configuration for the Telegram bridge.
#[derive(Debug, Clone)]
pub struct TelegramBridgeConfig {
    pub bot_token: String,
    pub chat_id: i64,
    pub mesh_channel: u32,
    pub direction: BridgeDirection,
    pub format: String, // e.g., "[{name}] {message}"
}

impl Default for TelegramBridgeConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            chat_id: 0,
            mesh_channel: 0,
            direction: BridgeDirection::Both,
            format: "[{name}] {message}".to_string(),
        }
    }
}

/// Telegram bridge instance.
pub struct TelegramBridge {
    config: TelegramBridgeConfig,
    bot: Bot,
}

fn render_mesh_message(format: &str, msg: &MeshBridgeMessage) -> String {
    format
        .replace("{name}", &msg.sender_name)
        .replace("{id}", &format!("!{:08x}", msg.sender_id))
        .replace("{message}", &msg.text)
        .replace("{channel}", &msg.channel.to_string())
}

impl TelegramBridge {
    /// Create a new Telegram bridge with the given configuration.
    pub fn new(config: TelegramBridgeConfig) -> Self {
        let bot = Bot::new(&config.bot_token);
        Self { config, bot }
    }

    /// Run the Telegram bridge.
    ///
    /// This spawns background tasks for both directions and runs until cancelled.
    pub async fn run(
        self,
        mesh_rx: MeshMessageReceiver,
        outgoing_tx: OutgoingMessageSender,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "Starting Telegram bridge (chat_id={}, direction={:?})",
            self.config.chat_id,
            self.config.direction
        );

        let config = Arc::new(self.config);
        let bot = self.bot;

        // Spawn mesh→telegram forwarder
        if config.direction.forwards_to_telegram() {
            let bot_clone = bot.clone();
            let config_clone = config.clone();
            let mesh_rx = mesh_rx;

            tokio::spawn(async move {
                Self::mesh_to_telegram_task(bot_clone, config_clone, mesh_rx).await;
            });
        }

        // Run telegram→mesh listener (this blocks)
        if config.direction.forwards_to_mesh() {
            Self::telegram_to_mesh_task(bot, config, outgoing_tx).await;
        } else {
            // If only mesh→telegram, just keep running
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        }

        Ok(())
    }

    /// Task that forwards mesh messages to Telegram.
    async fn mesh_to_telegram_task(
        bot: Bot,
        config: Arc<TelegramBridgeConfig>,
        mut mesh_rx: MeshMessageReceiver,
    ) {
        log::info!("Mesh→Telegram forwarder started");

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

                    let text = render_mesh_message(&config.format, &msg);

                    log::debug!("Forwarding to Telegram: {}", text);

                    if let Err(e) = bot
                        .send_message(ChatId(config.chat_id), &text)
                        .parse_mode(ParseMode::Html)
                        .await
                    {
                        log::error!("Failed to send to Telegram: {}", e);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("Telegram bridge lagged, missed {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("Mesh channel closed, stopping Telegram forwarder");
                    break;
                }
            }
        }
    }

    /// Task that forwards Telegram messages to mesh.
    async fn telegram_to_mesh_task(
        bot: Bot,
        config: Arc<TelegramBridgeConfig>,
        outgoing_tx: OutgoingMessageSender,
    ) {
        log::info!("Telegram→Mesh listener started");

        // Create a handler for incoming messages
        let handler = Update::filter_message().endpoint(
            move |_bot: Bot, msg: Message, config: Arc<TelegramBridgeConfig>, tx: mpsc::Sender<OutgoingBridgeMessage>| async move {
                // Only process messages from the configured chat
                if msg.chat.id.0 != config.chat_id {
                    return respond(());
                }

                // Get message text
                let text = match msg.text() {
                    Some(t) => t,
                    None => return respond(()), // Ignore non-text messages
                };

                // Get sender name
                let sender_name = msg
                    .from
                    .as_ref()
                    .map(|u| {
                        u.username
                            .clone()
                            .unwrap_or_else(|| u.first_name.clone())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                // Format message for mesh
                let mesh_text = format!("[TG:{}] {}", sender_name, text);

                // Check message length (Meshtastic limit ~230 bytes)
                let mesh_text = if mesh_text.len() > 220 {
                    format!("{}...", &mesh_text[..217])
                } else {
                    mesh_text
                };

                log::debug!("Forwarding to mesh: {}", mesh_text);

                // Send to mesh
                if let Err(e) = tx
                    .send(OutgoingBridgeMessage {
                        text: mesh_text,
                        channel: config.mesh_channel,
                        source: "telegram".to_string(),
                    })
                    .await
                {
                    log::error!("Failed to send to mesh: {}", e);
                }

                respond(())
            },
        );

        // Build dispatcher with dependencies
        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![config, outgoing_tx])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_direction_from_str() {
        assert_eq!(
            BridgeDirection::from_str("to_telegram"),
            BridgeDirection::ToTelegram
        );
        assert_eq!(
            BridgeDirection::from_str("mesh_to_telegram"),
            BridgeDirection::ToTelegram
        );
        assert_eq!(
            BridgeDirection::from_str("to_mesh"),
            BridgeDirection::ToMesh
        );
        assert_eq!(
            BridgeDirection::from_str("telegram_to_mesh"),
            BridgeDirection::ToMesh
        );
        assert_eq!(BridgeDirection::from_str("both"), BridgeDirection::Both);
        assert_eq!(BridgeDirection::from_str("unknown"), BridgeDirection::Both);
    }

    #[test]
    fn test_bridge_direction_forwards() {
        assert!(BridgeDirection::ToTelegram.forwards_to_telegram());
        assert!(!BridgeDirection::ToTelegram.forwards_to_mesh());

        assert!(!BridgeDirection::ToMesh.forwards_to_telegram());
        assert!(BridgeDirection::ToMesh.forwards_to_mesh());

        assert!(BridgeDirection::Both.forwards_to_telegram());
        assert!(BridgeDirection::Both.forwards_to_mesh());
    }

    #[test]
    fn test_format_mesh_message() {
        let msg = MeshBridgeMessage {
            sender_id: 0xaabbccdd,
            sender_name: "Alice".to_string(),
            text: "Hello world".to_string(),
            channel: 0,
            is_dm: false,
        };

        assert_eq!(
            render_mesh_message("[{name}] {message}", &msg),
            "[Alice] Hello world"
        );
    }

    #[test]
    fn test_format_mesh_message_with_id() {
        let msg = MeshBridgeMessage {
            sender_id: 0x12345678,
            sender_name: "Bob".to_string(),
            text: "Test".to_string(),
            channel: 0,
            is_dm: false,
        };

        assert_eq!(
            render_mesh_message("{id} ({name}): {message}", &msg),
            "!12345678 (Bob): Test"
        );
    }
}
