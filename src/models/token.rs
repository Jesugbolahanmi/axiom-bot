use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::alert::TopHolderItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenState {
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub description: Option<String>,
    pub dev_wallet: Option<String>,

    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,

    pub market_cap_sol: f64,
    pub market_cap_usd: f64,
    pub lowest_market_cap_usd: f64, // Track bottom / death trough market cap
    pub liquidity_sol: f64,
    pub liquidity_usd: f64,

    pub total_volume_sol: f64,
    pub total_volume_usd: f64,
    pub rolling_5m_volume_sol: f64,
    pub rolling_5m_volume_usd: f64,

    pub holder_count: u64,
    pub dev_holding_pct: f64,
    pub top_holder_pct: f64,
    pub sniper_bundler_pct: f64,

    pub has_lore: bool,
    pub alert_sent_at: Option<DateTime<Utc>>,

    // Top Holders and Bubblemaps Clusters
    pub top_holders: Vec<TopHolderItem>,
    pub top_10_pct: f64,
    pub clustered_wallets_count: usize,
}

impl TokenState {
    pub fn new(
        mint: String,
        name: String,
        symbol: String,
        uri: String,
        dev_wallet: Option<String>,
        market_cap_sol: f64,
        sol_price_usd: f64,
    ) -> Self {
        let now = Utc::now();
        let market_cap_usd = market_cap_sol * sol_price_usd;
        let liquidity_sol = market_cap_sol * 0.45;
        let liquidity_usd = liquidity_sol * sol_price_usd;

        Self {
            mint,
            name,
            symbol,
            uri,
            description: None,
            dev_wallet,
            created_at: now,
            last_updated: now,
            market_cap_sol,
            market_cap_usd,
            lowest_market_cap_usd: market_cap_usd,
            liquidity_sol,
            liquidity_usd,
            total_volume_sol: 0.0,
            total_volume_usd: 0.0,
            rolling_5m_volume_sol: 0.0,
            rolling_5m_volume_usd: 0.0,
            holder_count: 1,
            dev_holding_pct: 0.0,
            top_holder_pct: 0.0,
            sniper_bundler_pct: 0.0,
            has_lore: false,
            alert_sent_at: None,
            top_holders: Vec::new(),
            top_10_pct: 0.0,
            clustered_wallets_count: 0,
        }
    }

    pub fn age_mins(&self) -> f64 {
        let duration = Utc::now().signed_duration_since(self.created_at);
        duration.num_seconds() as f64 / 60.0
    }
}
