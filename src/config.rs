use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub bot: BotConfig,
    pub welcome: WelcomeConfig,
    pub weather: WeatherConfig,
    pub modules: HashMap<String, ModuleConfig>,
    #[serde(default)]
    pub bridge: BridgeConfig,
    #[serde(default)]
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Deserialize)]
pub struct DashboardConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_dashboard_bind")]
    pub bind_address: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_address: default_dashboard_bind(),
        }
    }
}

fn default_dashboard_bind() -> String {
    "0.0.0.0:9000".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct BridgeConfig {
    pub telegram: Option<TelegramConfig>,
    pub discord: Option<DiscordConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: i64,
    #[serde(default)]
    pub mesh_channel: u32,
    #[serde(default = "default_bridge_direction")]
    pub direction: String,
    #[serde(default = "default_telegram_format")]
    pub format: String,
}

fn default_bridge_direction() -> String {
    "both".to_string()
}

fn default_telegram_format() -> String {
    "[{name}] {message}".to_string()
}

fn default_discord_format() -> String {
    "**{name}**: {message}".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    pub bot_token: String,
    pub channel_id: u64,
    #[serde(default)]
    pub mesh_channel: u32,
    #[serde(default = "default_bridge_direction")]
    pub direction: String,
    #[serde(default = "default_discord_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct ConnectionConfig {
    pub address: String,
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_secs: u64,
}

fn default_reconnect_delay() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct BotConfig {
    pub name: String,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_command_prefix")]
    pub command_prefix: String,
    #[serde(default = "default_rate_limit_commands")]
    pub rate_limit_commands: usize,
    #[serde(default = "default_rate_limit_window")]
    pub rate_limit_window_secs: u64,
    #[serde(default = "default_send_delay_ms")]
    pub send_delay_ms: u64,
}

fn default_rate_limit_commands() -> usize {
    5
}

fn default_rate_limit_window() -> u64 {
    60
}

fn default_send_delay_ms() -> u64 {
    1500
}

fn default_command_prefix() -> String {
    "!".to_string()
}

fn default_db_path() -> String {
    "meshenger.db".to_string()
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct WelcomeConfig {
    pub enabled: bool,
    pub message: String,
    pub welcome_back_message: String,
    pub absence_threshold_hours: u64,
    #[serde(default)]
    pub whitelist: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub units: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ModuleConfig {
    pub enabled: bool,
    pub scope: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn is_module_enabled(&self, name: &str) -> bool {
        self.modules
            .get(name)
            .map(|m| m.enabled)
            .unwrap_or(false)
    }
}
