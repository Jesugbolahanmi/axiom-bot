pub mod thresholds;
pub mod safety;

use std::fs;
use std::path::Path;
use anyhow::{Context, Result};
use serde::Deserialize;
use thresholds::{FilterThresholds, RevivalThresholds};
use safety::SafetySettings;

#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_dry_run() -> bool { true }
fn default_poll_interval_secs() -> u64 { 5 }
fn default_log_level() -> String { "info".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    #[serde(default = "default_cooldown_mins")]
    pub alert_cooldown_mins: u64,
    #[serde(skip)]
    pub bot_token: String,
    #[serde(skip)]
    pub allowed_user_id: i64,
}

fn default_cooldown_mins() -> u64 { 60 }

#[derive(Debug, Clone, Deserialize)]
pub struct HeuristicsConfig {
    #[serde(default = "default_sniper_block_window")]
    pub sniper_block_window: u64,
    #[serde(default = "default_trace_funding_cluster")]
    pub trace_funding_cluster: bool,
}

fn default_sniper_block_window() -> u64 { 2 }
fn default_trace_funding_cluster() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
pub struct SourcesConfig {
    #[serde(default = "default_pumpportal_ws")]
    pub pumpportal_ws_url: String,
    #[serde(default = "default_pumpportal_trade")]
    pub pumpportal_trade_api: String,
    #[serde(skip)]
    pub helius_api_key: Option<String>,
    #[serde(skip)]
    pub helius_rpc_url: Option<String>,
    #[serde(skip)]
    pub solana_private_key: Option<String>,
}

fn default_pumpportal_ws() -> String {
    "wss://pumpportal.fun/api/data".to_string()
}

fn default_pumpportal_trade() -> String {
    "https://pumpportal.fun/api/trade-local".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub bot: BotConfig,
    pub telegram: TelegramConfig,
    pub filters: FilterThresholds,
    #[serde(default)]
    pub revival_filters: RevivalThresholds,
    pub heuristics: HeuristicsConfig,
    pub safety: SafetySettings,
    pub sources: SourcesConfig,
}

impl AppConfig {
    pub fn load_from_file_and_env<P: AsRef<Path>>(path: P) -> Result<Self> {
        let _ = dotenvy::dotenv();

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read configuration file: {}", path.as_ref().display()))?;

        let mut config: AppConfig = toml::from_str(&content)
            .with_context(|| "Failed to parse configuration TOML")?;

        // Merge sensitive / environment values
        config.telegram.bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .unwrap_or_default();

        config.telegram.allowed_user_id = std::env::var("TELEGRAM_ALLOWED_USER_ID")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        config.sources.helius_api_key = std::env::var("HELIUS_API_KEY").ok();
        config.sources.helius_rpc_url = std::env::var("HELIUS_RPC_URL").ok();
        config.sources.solana_private_key = std::env::var("SOLANA_PRIVATE_KEY").ok();

        Ok(config)
    }
}
