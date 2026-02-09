//! Bridge implementations for external platforms.

pub mod discord;
pub mod telegram;

pub use discord::{DiscordBridge, DiscordBridgeConfig};
pub use telegram::{BridgeDirection, TelegramBridge, TelegramBridgeConfig};
