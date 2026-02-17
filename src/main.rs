mod bot;
mod bridge;
mod bridges;
mod config;
mod dashboard;
mod db;
mod message;
mod module;
mod modules;
mod util;

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use bridge::create_bridge_channels;
use bridges::discord::BridgeDirection as DiscordDirection;
use bridges::{
    BridgeDirection, DiscordBridge, DiscordBridgeConfig, TelegramBridge, TelegramBridgeConfig,
};
use chrono::Local;
use config::Config;
use dashboard::Dashboard;
use db::Db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut logger = env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or("info,meshtastic::connections::stream_buffer=off"),
    );
    logger.format(|buf, record| {
        writeln!(
            buf,
            "[{} {} {}] {}",
            Local::now().format("%Y-%m-%dT%H:%M:%S%:z"),
            record.level(),
            record.target(),
            record.args()
        )
    });
    logger.init();

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

    let config = Arc::new(Config::load(path)?);
    let config_path_display = path
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| config_path.clone());
    log::info!(
        "Starting Meshenger (config={}, target={})",
        config_path_display,
        config.connection.address
    );

    let db_path = Path::new(&config.bot.db_path);
    if config.bot.db_path == ":memory:" {
        log::info!("Database mode: in-memory (:memory:)");
    } else {
        let existed = db_path.exists();
        if existed {
            log::info!(
                "Database mode: loading existing DB ({})",
                config.bot.db_path
            );
        } else {
            log::info!("Database mode: creating new DB ({})", config.bot.db_path);
        }
    }

    let db = Arc::new(Db::open(db_path)?);
    log::info!("Database opened at {}", config.bot.db_path);

    let registry = modules::build_registry(&config);
    log::info!("Registered {} module(s)", registry.all().len());

    // SSE broadcast channel for dashboard real-time updates
    let (sse_tx, _) = tokio::sync::broadcast::channel::<()>(16);

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
    let bot = bot::Bot::new(Arc::clone(&config), Arc::clone(&db), registry)
        .with_bridge_channels(bridge_tx, outgoing_rx)
        .with_sse_sender(sse_tx.clone());

    // Start dashboard if enabled
    if config.dashboard.enabled {
        let dashboard = Dashboard::new(
            Arc::clone(&config),
            Arc::clone(&db),
            bot.queue_depth(),
            bot.local_node_id(),
            sse_tx.clone(),
        );
        tokio::spawn(async move {
            if let Err(e) = dashboard.run().await {
                log::error!("Dashboard error: {}", e);
            }
        });
    }

    bot.run().await
}
