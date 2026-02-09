mod bot;
mod bridge;
mod bridges;
mod config;
mod db;
mod message;
mod module;
mod modules;
mod util;

use std::path::Path;

use bridge::create_bridge_channels;
use bridges::discord::BridgeDirection as DiscordDirection;
use bridges::{BridgeDirection, DiscordBridge, DiscordBridgeConfig, TelegramBridge, TelegramBridgeConfig};
use config::Config;
use db::Db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,meshtastic::connections::stream_buffer=off"),
    )
    .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let path = Path::new(&config_path);
    if !path.exists() {
        eprintln!("Config file not found: {}", config_path);
        eprintln!("Copy the example and edit it:");
        eprintln!("  cp config.example.toml config.toml");
        std::process::exit(1);
    }

    let config = Config::load(path)?;
    log::info!("Loaded config from {}", config_path);

    let db = Db::open(Path::new(&config.bot.db_path))?;
    log::info!("Database opened at {}", config.bot.db_path);

    let registry = modules::build_registry(&config);
    log::info!(
        "Registered {} module(s)",
        registry.all().len()
    );

    // Create bridge channels
    let (bridge_tx, outgoing_tx, outgoing_rx) = create_bridge_channels();

    // Start Telegram bridge if configured
    if let Some(telegram_config) = &config.bridge.telegram {
        if telegram_config.enabled {
            log::info!("Starting Telegram bridge...");

            let tg_config = TelegramBridgeConfig {
                bot_token: telegram_config.bot_token.clone(),
                chat_id: telegram_config.chat_id,
                mesh_channel: telegram_config.mesh_channel,
                direction: BridgeDirection::from_str(&telegram_config.direction),
                format: telegram_config.format.clone(),
            };

            let bridge = TelegramBridge::new(tg_config);
            let mesh_rx = bridge_tx.subscribe();
            let tx = outgoing_tx.clone();

            // Spawn bridge in background
            tokio::spawn(async move {
                if let Err(e) = bridge.run(mesh_rx, tx).await {
                    log::error!("Telegram bridge error: {}", e);
                }
            });
        }
    }

    // Start Discord bridge if configured
    if let Some(discord_config) = &config.bridge.discord {
        if discord_config.enabled {
            log::info!("Starting Discord bridge...");

            let dc_config = DiscordBridgeConfig {
                bot_token: discord_config.bot_token.clone(),
                channel_id: discord_config.channel_id,
                mesh_channel: discord_config.mesh_channel,
                direction: DiscordDirection::from_str(&discord_config.direction),
                format: discord_config.format.clone(),
            };

            let bridge = DiscordBridge::new(dc_config);
            let mesh_rx = bridge_tx.subscribe();
            let tx = outgoing_tx.clone();

            // Spawn bridge in background
            tokio::spawn(async move {
                if let Err(e) = bridge.run(mesh_rx, tx).await {
                    log::error!("Discord bridge error: {}", e);
                }
            });
        }
    }

    // Create bot with bridge channels
    let bot = bot::Bot::new(config, db, registry)
        .with_bridge_channels(bridge_tx, outgoing_rx);

    bot.run().await
}
