fn default_chain_solana() -> String {
    "solana".to_string()
}

use serde::{Deserialize, Serialize};
use super::token::TokenState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertType {
    EarlyLaunch,
    Revival,
    MultiChainRevival,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopHolderItem {
    pub rank: usize,
    pub address: String,
    pub amount: f64,
    pub pct: f64,
    pub is_dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCard {
    pub alert_type: AlertType,
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub age_mins: f64,
    pub market_cap_usd: f64,
    pub total_volume_usd: f64,
    pub rolling_5m_volume_usd: f64,
    pub volume_to_mc_ratio: f64,
    pub volume_5m_to_mc_ratio: f64,
    pub liquidity_usd: f64,
    pub liquidity_ratio: f64,
    pub holder_count: u64,
    pub dev_holding_pct: f64,
    pub top_holder_pct: f64,
    pub sniper_bundler_pct: f64,
    pub lore_description: Option<String>,
    pub has_lore: bool,

    // Revival metrics
    pub bottom_market_cap_usd: f64,
    pub surge_pct_from_bottom: f64,
    pub is_cto: bool,
    pub cto_status_desc: String,

    // Top Holders and Bubblemaps Clusters
    pub top_holders: Vec<TopHolderItem>,
    pub top_10_pct: f64,
    pub clustered_wallets_count: usize,
    pub bubblemaps_url: String,

    // Multi-chain fields
    #[serde(default = "default_chain_solana")]
    pub chain: String,
    #[serde(default)]
    pub sustained_secs: u64,
    #[serde(default)]
    pub buys_5m: u64,
    #[serde(default)]
    pub sells_5m: u64,
}

impl AlertCard {
    pub fn new_multi_chain_revival(
        chain: String,
        token_address: String,
        name: String,
        symbol: String,
        age_mins: f64,
        market_cap_usd: f64,
        rolling_5m_volume_usd: f64,
        total_volume_usd: f64,
        buys_5m: u64,
        sells_5m: u64,
        sustained_secs: u64,
        _dex_url: String,
        description: Option<String>,
    ) -> Self {
        let bubblemaps_url = if chain == "solana" {
            format!("https://bubblemaps.io/solana/token/{}", token_address)
        } else if chain == "bsc" {
            format!("https://bubblemaps.io/bsc/token/{}", token_address)
        } else if chain == "base" {
            format!("https://bubblemaps.io/base/token/{}", token_address)
        } else {
            format!("https://bubblemaps.io/eth/token/{}", token_address)
        };

        let has_lore = description.as_ref().map(|d| !d.is_empty()).unwrap_or(false);

        Self {
            alert_type: AlertType::MultiChainRevival,
            mint: token_address,
            name,
            symbol,
            age_mins,
            market_cap_usd,
            total_volume_usd,
            rolling_5m_volume_usd,
            volume_to_mc_ratio: if market_cap_usd > 0.0 { total_volume_usd / market_cap_usd } else { 0.0 },
            volume_5m_to_mc_ratio: if market_cap_usd > 0.0 { rolling_5m_volume_usd / market_cap_usd } else { 0.0 },
            liquidity_usd: market_cap_usd * 0.20,
            liquidity_ratio: 0.20,
            holder_count: (buys_5m + sells_5m).max(20),
            dev_holding_pct: 0.0,
            top_holder_pct: 0.0,
            sniper_bundler_pct: 0.0,
            lore_description: description,
            has_lore,
            bottom_market_cap_usd: 0.0,
            surge_pct_from_bottom: 0.0,
            is_cto: true,
            cto_status_desc: "Community Active / Multi-Chain".to_string(),
            top_holders: vec![],
            top_10_pct: 0.0,
            clustered_wallets_count: 0,
            bubblemaps_url,
            chain,
            sustained_secs,
            buys_5m,
            sells_5m,
        }
    }

    pub fn new_revival(token: &TokenState) -> Self {
        let mut card = Self::from(token);
        card.alert_type = AlertType::Revival;
        card
    }
}

impl From<&TokenState> for AlertCard {
    fn from(token: &TokenState) -> Self {
        let age_mins = token.age_mins();
        let volume_to_mc_ratio = if token.market_cap_usd > 0.0 {
            token.total_volume_usd / token.market_cap_usd
        } else {
            0.0
        };
        let volume_5m_to_mc_ratio = if token.market_cap_usd > 0.0 {
            token.rolling_5m_volume_usd / token.market_cap_usd
        } else {
            0.0
        };
        let liquidity_ratio = if token.market_cap_usd > 0.0 {
            token.liquidity_usd / token.market_cap_usd
        } else {
            0.0
        };
        let bubblemaps_url = format!("https://bubblemaps.io/solana/token/{}", token.mint);

        let bottom_mcap = if token.lowest_market_cap_usd > 0.0 {
            token.lowest_market_cap_usd
        } else {
            token.market_cap_usd
        };
        let surge_pct_from_bottom = if bottom_mcap > 0.0 {
            ((token.market_cap_usd - bottom_mcap) / bottom_mcap) * 100.0
        } else {
            0.0
        };

        let is_cto = token.dev_holding_pct <= 1.0;
        let cto_status_desc = if token.dev_holding_pct == 0.0 {
            "Dev Dumped (0.0% Clean CTO)".to_string()
        } else if is_cto {
            format!("Dev Out ({:.1}% CTO)", token.dev_holding_pct)
        } else {
            format!("Dev Holds {:.1}%", token.dev_holding_pct)
        };

        Self {
            alert_type: AlertType::EarlyLaunch,
            mint: token.mint.clone(),
            name: token.name.clone(),
            symbol: token.symbol.clone(),
            age_mins,
            market_cap_usd: token.market_cap_usd,
            total_volume_usd: token.total_volume_usd,
            rolling_5m_volume_usd: token.rolling_5m_volume_usd,
            volume_to_mc_ratio,
            volume_5m_to_mc_ratio,
            liquidity_usd: token.liquidity_usd,
            liquidity_ratio,
            holder_count: token.holder_count,
            dev_holding_pct: token.dev_holding_pct,
            top_holder_pct: token.top_holder_pct,
            sniper_bundler_pct: token.sniper_bundler_pct,
            lore_description: token.description.clone(),
            has_lore: token.has_lore,
            bottom_market_cap_usd: bottom_mcap,
            surge_pct_from_bottom,
            is_cto,
            cto_status_desc,
            top_holders: token.top_holders.clone(),
            top_10_pct: token.top_10_pct,
            clustered_wallets_count: token.clustered_wallets_count,
            bubblemaps_url,
            chain: "solana".to_string(),
            sustained_secs: 0,
            buys_5m: 0,
            sells_5m: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub alert_type: AlertType,
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub initial_market_cap_usd: f64,
    pub current_market_cap_usd: f64,
    pub highest_market_cap_usd: f64,
    pub initial_volume_usd: f64,
    pub current_volume_usd: f64,
    pub current_volume_5m_usd: f64,
    pub current_liquidity_usd: f64,
    pub holder_count: u64,
    pub dev_holding_pct: f64,
    pub top_holder_pct: f64,
    pub sniper_bundler_pct: f64,
    pub lore_description: Option<String>,
    pub alert_timestamp: chrono::DateTime<chrono::Utc>,
    pub last_refreshed: chrono::DateTime<chrono::Utc>,

    // Revival metrics
    pub bottom_market_cap_usd: f64,
    pub surge_pct_from_bottom: f64,
    pub is_cto: bool,
    pub cto_status_desc: String,

    // Top Holders and Bubblemaps Clusters
    pub top_holders: Vec<TopHolderItem>,
    pub top_10_pct: f64,
    pub clustered_wallets_count: usize,
    pub bubblemaps_url: String,

    // Continuous ATH & Milestone tracking
    #[serde(default = "chrono::Utc::now")]
    pub ath_timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub last_notified_milestone: f64,

    // Multi-chain fields
    #[serde(default = "default_chain_solana")]
    pub chain: String,
    #[serde(default)]
    pub sustained_secs: u64,
    #[serde(default)]
    pub buys_5m: u64,
    #[serde(default)]
    pub sells_5m: u64,
}

impl From<&AlertCard> for CallRecord {
    fn from(card: &AlertCard) -> Self {
        let now = chrono::Utc::now();
        Self {
            alert_type: card.alert_type.clone(),
            mint: card.mint.clone(),
            name: card.name.clone(),
            symbol: card.symbol.clone(),
            initial_market_cap_usd: card.market_cap_usd,
            current_market_cap_usd: card.market_cap_usd,
            highest_market_cap_usd: card.market_cap_usd,
            initial_volume_usd: card.total_volume_usd,
            current_volume_usd: card.total_volume_usd,
            current_volume_5m_usd: card.rolling_5m_volume_usd,
            current_liquidity_usd: card.liquidity_usd,
            holder_count: card.holder_count,
            dev_holding_pct: card.dev_holding_pct,
            top_holder_pct: card.top_holder_pct,
            sniper_bundler_pct: card.sniper_bundler_pct,
            lore_description: card.lore_description.clone(),
            alert_timestamp: now,
            last_refreshed: now,
            bottom_market_cap_usd: card.bottom_market_cap_usd,
            surge_pct_from_bottom: card.surge_pct_from_bottom,
            is_cto: card.is_cto,
            cto_status_desc: card.cto_status_desc.clone(),
            top_holders: card.top_holders.clone(),
            top_10_pct: card.top_10_pct,
            clustered_wallets_count: card.clustered_wallets_count,
            bubblemaps_url: card.bubblemaps_url.clone(),
            ath_timestamp: now,
            last_notified_milestone: 1.0,
            chain: card.chain.clone(),
            sustained_secs: card.sustained_secs,
            buys_5m: card.buys_5m,
            sells_5m: card.sells_5m,
        }
    }
}
