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
    pub pair_address: Option<String>,
    pub peak_candle_mcap_usd: Option<f64>,
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
    #[serde(rename = "pairAddress")]
    pair_address: Option<String>,
    #[serde(rename = "dexId")]
    #[allow(dead_code)]
    dex_id: Option<String>,
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
    #[allow(dead_code)]
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

    /// Queries GeckoTerminal 5-minute OHLCV candles for the pool address strictly AFTER alert_epoch.
    /// This prevents attributing price action that occurred before the call to the token's ATH.
    pub async fn fetch_pool_candle_peak_after(&self, pool_address: &str, alert_epoch: i64) -> Option<f64> {
        let url = format!(
            "https://api.geckoterminal.com/api/v2/networks/solana/pools/{}/ohlcv/minute?aggregate=5&limit=288",
            pool_address
        );
        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let body: serde_json::Value = resp.json().await.ok()?;
        let candles = body.get("data")?.get("attributes")?.get("ohlcv_list")?.as_array()?;
        let mut max_high = 0.0;
        for c in candles {
            // Candle format: [timestamp, open, high, low, close, volume]
            let ts = c.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
            // STRICT FILTER: Only consider candles that started at or after the call time
            if ts < (alert_epoch - 300) {
                continue;
            }
            if let Some(h) = c.get(2).and_then(|v| v.as_f64()) {
                if h > max_high {
                    max_high = h;
                }
            }
        }

        if max_high > 0.0 {
            // Pump.fun standard supply 1,000,000,000 tokens
            Some(max_high * 1_000_000_000.0)
        } else {
            None
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

        // Filter to pairs with a valid market cap or fdv > 0
        let valid_pairs: Vec<&DexPair> = pairs
            .iter()
            .filter(|p| p.market_cap.or(p.fdv).unwrap_or(0.0) > 0.0)
            .collect();

        if valid_pairs.is_empty() {
            return Ok(None);
        }

        // Canonical pair selection:
        // Pick pair with highest 24h trading volume (correctly captures pump.fun bonding curves where liquidity is None)
        let best_pair = valid_pairs
            .iter()
            .max_by(|a, b| {
                let vol_a = a.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                let vol_b = b.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                match vol_a.partial_cmp(&vol_b) {
                    Some(std::cmp::Ordering::Equal) | None => {
                        let m_a = a.market_cap.or(a.fdv).unwrap_or(0.0);
                        let m_b = b.market_cap.or(b.fdv).unwrap_or(0.0);
                        m_a.partial_cmp(&m_b).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    Some(order) => order,
                }
            })
            .copied()
            .unwrap_or(valid_pairs[0]);

        // Current live market cap of the primary active trading pair
        let mcap = best_pair
            .market_cap
            .or(best_pair.fdv)
            .unwrap_or(0.0);

        // Liquidity: use pair liquidity if available, otherwise virtual bonding curve (~40% of mcap)
        let liq = best_pair
            .liquidity
            .as_ref()
            .and_then(|l| l.usd)
            .unwrap_or(mcap * 0.40);

        let vol_24h = valid_pairs
            .iter()
            .filter_map(|p| p.volume.as_ref().and_then(|v| v.h24))
            .sum::<f64>()
            .max(best_pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0));

        let vol_5m = valid_pairs
            .iter()
            .filter_map(|p| p.volume.as_ref().and_then(|v| v.m5))
            .sum::<f64>()
            .max(best_pair.volume.as_ref().and_then(|v| v.m5).unwrap_or(0.0));

        let base_name = best_pair.base_token.as_ref().and_then(|t| t.name.clone());
        let base_sym = best_pair.base_token.as_ref().and_then(|t| t.symbol.clone());

        Ok(Some(DexMetrics {
            base_token_name: base_name,
            base_token_symbol: base_sym,
            market_cap_usd: mcap,
            liquidity_usd: liq,
            total_volume_usd: vol_24h,
            volume_5m_usd: vol_5m,
            pair_address: best_pair.pair_address.clone(),
            peak_candle_mcap_usd: None,
        }))
    }
}
