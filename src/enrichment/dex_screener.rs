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


/// Full live snapshot of a token on any DexScreener-supported chain (used for CA lookups).
#[derive(Debug, Clone, Default)]
pub struct TokenOverview {
    pub chain: String,
    pub token_address: String,
    pub dex_id: String,
    pub pair_address: String,
    pub url: String,
    pub name: String,
    pub symbol: String,
    pub price_usd: f64,
    pub market_cap_usd: f64,
    pub fdv_usd: f64,
    pub liquidity_usd: f64,
    pub vol_5m: f64,
    pub vol_1h: f64,
    pub vol_6h: f64,
    pub vol_24h: f64,
    pub buys_5m: u64,
    pub sells_5m: u64,
    pub buys_1h: u64,
    pub sells_1h: u64,
    pub buys_24h: u64,
    pub sells_24h: u64,
    pub chg_5m: f64,
    pub chg_1h: f64,
    pub chg_6h: f64,
    pub chg_24h: f64,
    pub pair_created_at_ms: Option<i64>,
    pub websites: Vec<String>,
    pub socials: Vec<(String, String)>,
    pub pair_count: usize,
}

fn jf(v: &serde_json::Value, ptr: &str) -> f64 {
    match v.pointer(ptr) {
        Some(x) => x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse::<f64>().ok())).unwrap_or(0.0),
        None => 0.0,
    }
}

fn ju(v: &serde_json::Value, ptr: &str) -> u64 {
    v.pointer(ptr).and_then(|x| x.as_u64()).unwrap_or(0)
}

fn js(v: &serde_json::Value, ptr: &str) -> String {
    v.pointer(ptr).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

impl DexScreenerClient {
    async fn get_pairs_json(&self, url: &str) -> Vec<serde_json::Value> {
        let resp = match self.http_client.get(url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => return Vec::new(),
        };
        match resp.json::<serde_json::Value>().await {
            Ok(body) => body.get("pairs").and_then(|p| p.as_array()).cloned().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Resolve any contract address (token or pair, any chain) into a live token overview.
    pub async fn fetch_token_overview(&self, address: &str) -> Option<TokenOverview> {
        let addr = address.trim();
        let mut pairs = self
            .get_pairs_json(&format!("https://api.dexscreener.com/latest/dex/tokens/{}", addr))
            .await;

        // Fallback: the address may be a pair/pool address (e.g. pasted from a DexScreener URL)
        if pairs.is_empty() {
            pairs = self
                .get_pairs_json(&format!("https://api.dexscreener.com/latest/dex/search?q={}", addr))
                .await
                .into_iter()
                .filter(|p| {
                    js(p, "/pairAddress").eq_ignore_ascii_case(addr)
                        || js(p, "/baseToken/address").eq_ignore_ascii_case(addr)
                })
                .collect();
        }
        if pairs.is_empty() {
            return None;
        }

        // Which token are we describing? Prefer the queried address as base token,
        // otherwise (pair address pasted) use that pair's base token.
        let token_address = pairs
            .iter()
            .map(|p| js(p, "/baseToken/address"))
            .find(|a| a.eq_ignore_ascii_case(addr))
            .unwrap_or_else(|| js(&pairs[0], "/baseToken/address"));

        let token_pairs: Vec<&serde_json::Value> = pairs
            .iter()
            .filter(|p| js(p, "/baseToken/address").eq_ignore_ascii_case(&token_address))
            .collect();
        let token_pairs = if token_pairs.is_empty() { pairs.iter().collect() } else { token_pairs };

        let best = token_pairs
            .iter()
            .max_by(|a, b| {
                let va = jf(a, "/volume/h24");
                let vb = jf(b, "/volume/h24");
                match va.partial_cmp(&vb) {
                    Some(std::cmp::Ordering::Equal) | None => jf(a, "/liquidity/usd")
                        .partial_cmp(&jf(b, "/liquidity/usd"))
                        .unwrap_or(std::cmp::Ordering::Equal),
                    Some(o) => o,
                }
            })
            .copied()?;

        let mcap = {
            let m = jf(best, "/marketCap");
            if m > 0.0 { m } else { jf(best, "/fdv") }
        };

        let websites = best
            .pointer("/info/websites")
            .and_then(|w| w.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.get("url").and_then(|u| u.as_str()).map(String::from)).collect())
            .unwrap_or_default();
        let socials = best
            .pointer("/info/socials")
            .and_then(|w| w.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| {
                        let url = x.get("url").and_then(|u| u.as_str())?;
                        let kind = x.get("type").and_then(|t| t.as_str()).unwrap_or("link");
                        Some((kind.to_string(), url.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(TokenOverview {
            chain: js(best, "/chainId").to_lowercase(),
            token_address: if token_address.is_empty() { addr.to_string() } else { token_address },
            dex_id: js(best, "/dexId"),
            pair_address: js(best, "/pairAddress"),
            url: js(best, "/url"),
            name: js(best, "/baseToken/name"),
            symbol: js(best, "/baseToken/symbol"),
            price_usd: jf(best, "/priceUsd"),
            market_cap_usd: mcap,
            fdv_usd: jf(best, "/fdv"),
            liquidity_usd: jf(best, "/liquidity/usd"),
            vol_5m: jf(best, "/volume/m5"),
            vol_1h: jf(best, "/volume/h1"),
            vol_6h: jf(best, "/volume/h6"),
            vol_24h: jf(best, "/volume/h24"),
            buys_5m: ju(best, "/txns/m5/buys"),
            sells_5m: ju(best, "/txns/m5/sells"),
            buys_1h: ju(best, "/txns/h1/buys"),
            sells_1h: ju(best, "/txns/h1/sells"),
            buys_24h: ju(best, "/txns/h24/buys"),
            sells_24h: ju(best, "/txns/h24/sells"),
            chg_5m: jf(best, "/priceChange/m5"),
            chg_1h: jf(best, "/priceChange/h1"),
            chg_6h: jf(best, "/priceChange/h6"),
            chg_24h: jf(best, "/priceChange/h24"),
            pair_created_at_ms: best.get("pairCreatedAt").and_then(|v| v.as_i64()),
            websites,
            socials,
            pair_count: token_pairs.len(),
        })
    }
}
