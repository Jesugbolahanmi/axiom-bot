use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct FilterThresholds {
    #[serde(default = "default_min_age_mins")]
    pub min_age_mins: u64,
    #[serde(default = "default_max_age_mins")]
    pub max_age_mins: u64,

    #[serde(default = "default_min_mcap_usd")]
    pub min_mcap_usd: f64,
    #[serde(default = "default_max_mcap_usd")]
    pub max_mcap_usd: f64,

    #[serde(default = "default_min_total_volume_mult")]
    pub min_total_volume_mult: f64,
    #[serde(default = "default_min_5m_volume_ratio")]
    pub min_5m_volume_ratio: f64,

    #[serde(default = "default_min_liquidity_ratio")]
    pub min_liquidity_ratio: f64,

    #[serde(default = "default_min_holders")]
    pub min_holders: u64,
    #[serde(default = "default_max_dev_holding_pct")]
    pub max_dev_holding_pct: f64,
    #[serde(default = "default_max_top_holder_pct")]
    pub max_top_holder_pct: f64,
    #[serde(default = "default_max_sniper_bundler_pct")]
    pub max_sniper_bundler_pct: f64,

    #[serde(default = "default_min_lore_length")]
    pub min_lore_length: usize,
}

fn default_min_age_mins() -> u64 { 5 }
fn default_max_age_mins() -> u64 { 60 }
fn default_min_mcap_usd() -> f64 { 10_000.0 }
fn default_max_mcap_usd() -> f64 { 35_000.0 }
fn default_min_total_volume_mult() -> f64 { 1.0 }
fn default_min_5m_volume_ratio() -> f64 { 0.10 }
fn default_min_liquidity_ratio() -> f64 { 0.40 }
fn default_min_holders() -> u64 { 50 }
fn default_max_dev_holding_pct() -> f64 { 15.0 }
fn default_max_top_holder_pct() -> f64 { 15.0 }
fn default_max_sniper_bundler_pct() -> f64 { 15.0 }
fn default_min_lore_length() -> usize { 20 }

impl Default for FilterThresholds {
    fn default() -> Self {
        Self {
            min_age_mins: default_min_age_mins(),
            max_age_mins: default_max_age_mins(),
            min_mcap_usd: default_min_mcap_usd(),
            max_mcap_usd: default_max_mcap_usd(),
            min_total_volume_mult: default_min_total_volume_mult(),
            min_5m_volume_ratio: default_min_5m_volume_ratio(),
            min_liquidity_ratio: default_min_liquidity_ratio(),
            min_holders: default_min_holders(),
            max_dev_holding_pct: default_max_dev_holding_pct(),
            max_top_holder_pct: default_max_top_holder_pct(),
            max_sniper_bundler_pct: default_max_sniper_bundler_pct(),
            min_lore_length: default_min_lore_length(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevivalThresholds {
    #[serde(default = "default_revival_enabled")]
    pub enabled: bool,
    #[serde(default = "default_revival_min_age_mins")]
    pub min_age_mins: u64,
    #[serde(default = "default_revival_max_age_mins")]
    pub max_age_mins: u64,
    #[serde(default = "default_revival_max_death_mcap_usd")]
    pub max_death_mcap_usd: f64,
    #[serde(default = "default_revival_min_surge_pct")]
    pub min_surge_from_death_pct: f64,
    #[serde(default = "default_revival_min_5m_volume_usd")]
    pub min_5m_volume_usd: f64,
    #[serde(default = "default_revival_max_dev_holding_pct")]
    pub max_dev_holding_pct: f64,
    #[serde(default = "default_revival_min_holders")]
    pub min_holders: u64,
    #[serde(default = "default_revival_max_mcap_usd")]
    pub max_mcap_usd: f64,
}

fn default_revival_max_mcap_usd() -> f64 { 100_000.0 }
fn default_revival_enabled() -> bool { true }
fn default_revival_min_age_mins() -> u64 { 60 }
fn default_revival_max_age_mins() -> u64 { 2880 }
fn default_revival_max_death_mcap_usd() -> f64 { 6_000.0 }
fn default_revival_min_surge_pct() -> f64 { 50.0 }
fn default_revival_min_5m_volume_usd() -> f64 { 1_000.0 }
fn default_revival_max_dev_holding_pct() -> f64 { 1.0 }
fn default_revival_min_holders() -> u64 { 30 }

impl Default for RevivalThresholds {
    fn default() -> Self {
        Self {
            enabled: default_revival_enabled(),
            min_age_mins: default_revival_min_age_mins(),
            max_age_mins: default_revival_max_age_mins(),
            max_death_mcap_usd: default_revival_max_death_mcap_usd(),
            min_surge_from_death_pct: default_revival_min_surge_pct(),
            min_5m_volume_usd: default_revival_min_5m_volume_usd(),
            max_dev_holding_pct: default_revival_max_dev_holding_pct(),
            min_holders: default_revival_min_holders(),
            max_mcap_usd: default_revival_max_mcap_usd(),
        }
    }
}


#[derive(Debug, Clone, Deserialize)]
pub struct MultiChainRevivalConfig {
    #[serde(default = "default_mcr_enabled")]
    pub enabled: bool,
    #[serde(default = "default_mcr_chains")]
    pub chains: Vec<String>,
    #[serde(default = "default_mcr_min_age_mins")]
    pub min_age_mins: u64,
    #[serde(default = "default_mcr_min_5m_volume_usd")]
    pub min_5m_volume_usd: f64,
    #[serde(default = "default_mcr_min_buy_ratio")]
    pub min_buy_ratio: f64,
    #[serde(default = "default_mcr_min_sustained_secs")]
    pub min_sustained_secs: u64,
    #[serde(default = "default_mcr_candidate_ttl_secs")]
    pub candidate_ttl_secs: u64,
    #[serde(default = "default_mcr_poll_interval_secs")]
    pub poll_interval_secs: u64,
}

fn default_mcr_enabled() -> bool { true }
fn default_mcr_chains() -> Vec<String> {
    vec!["solana".to_string(), "base".to_string(), "ethereum".to_string(), "bsc".to_string()]
}
fn default_mcr_min_age_mins() -> u64 { 60 }
fn default_mcr_min_5m_volume_usd() -> f64 { 1500.0 }
fn default_mcr_min_buy_ratio() -> f64 { 0.60 }
fn default_mcr_min_sustained_secs() -> u64 { 120 }
fn default_mcr_candidate_ttl_secs() -> u64 { 300 }
fn default_mcr_poll_interval_secs() -> u64 { 20 }

impl Default for MultiChainRevivalConfig {
    fn default() -> Self {
        Self {
            enabled: default_mcr_enabled(),
            chains: default_mcr_chains(),
            min_age_mins: default_mcr_min_age_mins(),
            min_5m_volume_usd: default_mcr_min_5m_volume_usd(),
            min_buy_ratio: default_mcr_min_buy_ratio(),
            min_sustained_secs: default_mcr_min_sustained_secs(),
            candidate_ttl_secs: default_mcr_candidate_ttl_secs(),
            poll_interval_secs: default_mcr_poll_interval_secs(),
        }
    }
}
