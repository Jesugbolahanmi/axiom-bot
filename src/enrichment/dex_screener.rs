use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, Default)]
pub struct DexMetrics {
    pub base_token_name: Option<String>,
    pub base_token_symbol: Option<String>,
    pub market_cap_usd: f64,
    pub liquidity_usd: f64,
    pub total_volume_usd: f64,
    pub volume_5m_usd: f64,
}

#[derive(Debug, Deserialize)]
struct DexResponse {
    pairs: Option<Vec<DexPair>>,
}

#[derive(Debug, Deserialize)]
struct DexToken {
    name: Option<String>,
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DexPair {
    #[serde(rename = "baseToken")]
    base_token: Option<DexToken>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    fdv: Option<f64>,
    volume: Option<DexVolume>,
    liquidity: Option<DexLiquidity>,
}

#[derive(Debug, Deserialize)]
struct DexVolume {
    m5: Option<f64>,
    h1: Option<f64>,
    h24: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DexLiquidity {
    usd: Option<f64>,
}

#[derive(Clone)]
pub struct DexScreenerClient {
    http_client: Client,
}

impl DexScreenerClient {
    pub fn new() -> Self {
        Self {
            http_client: Client::builder()
                .timeout(Duration::from_secs(6))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch live market cap, volume, and liquidity from DexScreener
    pub async fn fetch_metrics(&self, mint: &str) -> Result<Option<DexMetrics>> {
        let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", mint);
        let resp = match self.http_client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                debug!("DexScreener request error for {}: {:?}", mint, e);
                return Ok(None);
            }
        };

        if !resp.status().is_success() {
            return Ok(None);
        }

        let body: DexResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                debug!("Failed to parse DexScreener response for {}: {:?}", mint, e);
                return Ok(None);
            }
        };

        let pairs = match body.pairs {
            Some(p) if !p.is_empty() => p,
            _ => return Ok(None),
        };

        // Pick pair with highest liquidity or first pair
        let best_pair = pairs
            .iter()
            .max_by(|a, b| {
                let liq_a = a.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                let liq_b = b.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                liq_a.partial_cmp(&liq_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(&pairs[0]);

        let mcap = best_pair
            .market_cap
            .or(best_pair.fdv)
            .unwrap_or(0.0);

        let liq = best_pair
            .liquidity
            .as_ref()
            .and_then(|l| l.usd)
            .unwrap_or(mcap * 0.45);

        let vol_24h = best_pair
            .volume
            .as_ref()
            .and_then(|v| v.h24.or(v.h1))
            .unwrap_or(0.0);

        let vol_5m = best_pair
            .volume
            .as_ref()
            .and_then(|v| v.m5)
            .unwrap_or(0.0);

        let base_name = best_pair.base_token.as_ref().and_then(|t| t.name.clone());
        let base_sym = best_pair.base_token.as_ref().and_then(|t| t.symbol.clone());

        Ok(Some(DexMetrics {
            base_token_name: base_name,
            base_token_symbol: base_sym,
            market_cap_usd: mcap,
            liquidity_usd: liq,
            total_volume_usd: vol_24h,
            volume_5m_usd: vol_5m,
        }))
    }
}
