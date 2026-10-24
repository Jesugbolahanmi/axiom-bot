use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::alerts::TelegramBotService;
use crate::config::thresholds::MultiChainRevivalConfig;
use crate::models::alert::AlertCard;

#[derive(Debug, Clone)]
pub struct RevivalCandidate {
    pub chain: String,
    pub token_address: String,
    pub name: String,
    pub symbol: String,
    pub pair_address: Option<String>,
    pub age_mins: f64,
    pub market_cap_usd: f64,
    pub total_volume_usd: f64,
    pub volume_5m_usd: f64,
    pub buys_5m: u64,
    pub sells_5m: u64,
    pub first_detected_at: chrono::DateTime<chrono::Utc>,
    pub last_active_at: chrono::DateTime<chrono::Utc>,
    pub alerted: bool,
    pub dex_url: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ProfileItem {
    #[serde(rename = "chainId")]
    chain_id: Option<String>,
    #[serde(rename = "tokenAddress")]
    token_address: Option<String>,
    url: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct BoostItem {
    #[serde(rename = "chainId")]
    chain_id: Option<String>,
    #[serde(rename = "tokenAddress")]
    token_address: Option<String>,
    url: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DexTokensResponse {
    pairs: Option<Vec<DexPairInfo>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DexPairInfo {
    #[serde(rename = "chainId")]
    chain_id: Option<String>,
    #[serde(rename = "pairAddress")]
    pair_address: Option<String>,
    url: Option<String>,
    #[serde(rename = "baseToken")]
    base_token: Option<DexTokenMeta>,
    #[serde(rename = "marketCap")]
    market_cap: Option<f64>,
    fdv: Option<f64>,
    volume: Option<DexVolM5>,
    txns: Option<DexTxnsM5>,
    #[serde(rename = "pairCreatedAt")]
    pair_created_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct DexTokenMeta {
    name: Option<String>,
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DexVolM5 {
    m5: Option<f64>,
    h24: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct DexTxnsM5 {
    m5: Option<DexTxCount>,
}

#[derive(Debug, Deserialize)]
pub struct DexTxCount {
    buys: Option<u64>,
    sells: Option<u64>,
}

pub struct MultiChainRevivalScanner {
    config: MultiChainRevivalConfig,
    http_client: Client,
    candidates: Arc<Mutex<HashMap<String, RevivalCandidate>>>,
    telegram: Option<Arc<TelegramBotService>>,
}

impl MultiChainRevivalScanner {
    pub fn new(
        config: MultiChainRevivalConfig,
        telegram: Option<Arc<TelegramBotService>>,
    ) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(6))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AxiomBot/2.0")
            .build()
            .unwrap_or_default();

        Self {
            config,
            http_client,
            candidates: Arc::new(Mutex::new(HashMap::new())),
            telegram,
        }
    }

    /// Check if chain is configured to be monitored
    pub fn is_supported_chain(&self, chain: &str) -> bool {
        let c = chain.to_lowercase();
        self.config.chains.iter().any(|allowed| allowed.to_lowercase() == c)
    }

    /// Fetch latest token addresses from DexScreener profiles & boosts
    async fn discover_candidate_addresses(&self) -> Vec<(String, String, Option<String>)> {
        let mut results = Vec::new();

        // 1. Fetch latest token profiles
        if let Ok(resp) = self.http_client.get("https://api.dexscreener.com/token-profiles/latest/v1").send().await {
            if let Ok(items) = resp.json::<Vec<ProfileItem>>().await {
                for item in items {
                    if let (Some(chain), Some(addr)) = (item.chain_id, item.token_address) {
                        if self.is_supported_chain(&chain) {
                            results.push((chain.to_lowercase(), addr, item.description));
                        }
                    }
                }
            }
        }

        // 2. Fetch latest boosted tokens
        if let Ok(resp) = self.http_client.get("https://api.dexscreener.com/token-boosts/latest/v1").send().await {
            if let Ok(items) = resp.json::<Vec<BoostItem>>().await {
                for item in items {
                    if let (Some(chain), Some(addr)) = (item.chain_id, item.token_address) {
                        if self.is_supported_chain(&chain) {
                            results.push((chain.to_lowercase(), addr, item.description));
                        }
                    }
                }
            }
        }

        // Deduplicate by address
        let mut seen = std::collections::HashSet::new();
        let mut unique = Vec::new();
        for (chain, addr, desc) in results {
            let key = format!("{}:{}", chain, addr.to_lowercase());
            if seen.insert(key) {
                unique.push((chain, addr, desc));
            }
        }

        unique
    }

    /// Fetch live token pair metrics across any chain
    pub async fn fetch_token_pair_data(&self, token_address: &str) -> Option<DexPairInfo> {
        let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", token_address);
        let resp = self.http_client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }

        let body = resp.json::<DexTokensResponse>().await.ok()?;
        let pairs = body.pairs?;
        if pairs.is_empty() {
            return None;
        }

        // Pick pair with highest 24h/5m volume
        pairs.into_iter().max_by(|a, b| {
            let va = a.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
            let vb = b.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Evaluate candidate and handle the sustained volume confirmation state machine
    pub async fn evaluate_and_track_candidate(
        &self,
        chain: String,
        token_address: String,
        pair: DexPairInfo,
        description: Option<String>,
    ) -> Option<AlertCard> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let created_ms = pair.pair_created_at.unwrap_or(now_ms);
        let age_mins = ((now_ms - created_ms).max(0) as f64) / 60_000.0;

        // Must be at least min_age_mins old (dormant/established token, not newly minted seconds ago)
        if age_mins < (self.config.min_age_mins as f64) {
            debug!("Token {} age {:.1}m < min_age_mins {}", token_address, age_mins, self.config.min_age_mins);
            return None;
        }

        let vol_5m = pair.volume.as_ref().and_then(|v| v.m5).unwrap_or(0.0);
        let vol_24h = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(vol_5m);
        let mcap = pair.market_cap.or(pair.fdv).unwrap_or(0.0);

        let txns = pair.txns.as_ref().and_then(|t| t.m5.as_ref());
        let buys = txns.and_then(|t| t.buys).unwrap_or(0);
        let sells = txns.and_then(|t| t.sells).unwrap_or(0);
        let total_tx = buys + sells;

        // Threshold check 1: 5-minute volume >= min_5m_volume_usd (default $1,500)
        if vol_5m < self.config.min_5m_volume_usd {
            return None;
        }

        // Threshold check 2: More buys than sells (buy ratio >= min_buy_ratio, with at least 5 buys)
        if total_tx < 5 || buys == 0 {
            return None;
        }
        let buy_ratio = (buys as f64) / (total_tx as f64);
        if buy_ratio < self.config.min_buy_ratio {
            debug!("Token {} buy ratio {:.2} < min_buy_ratio {}", token_address, buy_ratio, self.config.min_buy_ratio);
            return None;
        }

        let name = pair.base_token.as_ref().and_then(|t| t.name.clone()).unwrap_or_else(|| "Unknown Token".to_string());
        let symbol = pair.base_token.as_ref().and_then(|t| t.symbol.clone()).unwrap_or_else(|| "TOKEN".to_string());
        let dex_url = pair.url.unwrap_or_else(|| format!("https://dexscreener.com/{}/{}", chain, token_address));

        let candidate_key = format!("{}:{}", chain, token_address.to_lowercase());
        let now = chrono::Utc::now();
        let mut candidates_guard = self.candidates.lock().await;

        if let Some(candidate) = candidates_guard.get_mut(&candidate_key) {
            // Existing candidate: update latest active metrics
            candidate.last_active_at = now;
            candidate.market_cap_usd = mcap;
            candidate.volume_5m_usd = vol_5m;
            candidate.total_volume_usd = vol_24h;
            candidate.buys_5m = buys;
            candidate.sells_5m = sells;

            let sustained_secs = (now - candidate.first_detected_at).num_seconds().max(0) as u64;

            // SUSTAINED VOLUME CHECK: Must stay alive for at least min_sustained_secs (default 120s / 2m)
            if sustained_secs >= self.config.min_sustained_secs && !candidate.alerted {
                candidate.alerted = true;
                info!(
                    "🔥 [SUSTAINED REVIVAL CONFIRMED] ${} ({}) on [{}] - Volume sustained for {}s (5m Vol: ${:.0}, Buys: {}/{}, MCap: ${:.0})",
                    symbol, token_address, chain.to_uppercase(), sustained_secs, vol_5m, buys, total_tx, mcap
                );

                let card = AlertCard::new_multi_chain_revival(
                    chain,
                    token_address,
                    name,
                    symbol,
                    age_mins,
                    mcap,
                    vol_5m,
                    vol_24h,
                    buys,
                    sells,
                    sustained_secs,
                    dex_url,
                    description,
                );
                return Some(card);
            } else if !candidate.alerted {
                info!(
                    "⏳ [REVIVAL MATURING] ${} ({}) [{}] - Sustained: {}s / {}s required (5m Vol: ${:.0}, Buys: {}/{})",
                    symbol, token_address, chain.to_uppercase(), sustained_secs, self.config.min_sustained_secs, vol_5m, buys, total_tx
                );
            }
        } else {
            // New candidate discovery: begin sustained volume clock
            info!(
                "🔍 [REVIVAL CANDIDATE] ${} ({}) on [{}] detected! Age: {:.1}h, 5m Vol: ${:.0}, Buys: {}/{} ({:.0}%). Starting {}s sustained check...",
                symbol, token_address, chain.to_uppercase(), age_mins / 60.0, vol_5m, buys, total_tx, buy_ratio * 100.0, self.config.min_sustained_secs
            );

            candidates_guard.insert(
                candidate_key,
                RevivalCandidate {
                    chain,
                    token_address,
                    name,
                    symbol,
                    pair_address: pair.pair_address,
                    age_mins,
                    market_cap_usd: mcap,
                    total_volume_usd: vol_24h,
                    volume_5m_usd: vol_5m,
                    buys_5m: buys,
                    sells_5m: sells,
                    first_detected_at: now,
                    last_active_at: now,
                    alerted: false,
                    dex_url,
                    description,
                },
            );
        }

        None
    }

    /// Prune dead candidates that stopped trading before hitting sustained threshold
    pub async fn prune_stale_candidates(&self) {
        let now = chrono::Utc::now();
        let ttl_secs = self.config.candidate_ttl_secs as i64;
        let mut guard = self.candidates.lock().await;
        let before_count = guard.len();
        guard.retain(|_key, candidate| {
            let idle_secs = (now - candidate.last_active_at).num_seconds();
            idle_secs < ttl_secs
        });
        let pruned = before_count.saturating_sub(guard.len());
        if pruned > 0 {
            debug!("Pruned {} stale multi-chain revival candidates (idle > {}s)", pruned, ttl_secs);
        }
    }

    /// Continuous background scanner loop
    pub async fn run(&self) {
        if !self.config.enabled {
            info!("Multi-chain revival scanner is disabled in configuration.");
            return;
        }

        info!(
            "🌐 Multi-Chain Revival Scanner running across chains {:?} (Min Age: {}m, Min 5m Vol: ${:.0}, Sustained Window: {}s)",
            self.config.chains, self.config.min_age_mins, self.config.min_5m_volume_usd, self.config.min_sustained_secs
        );

        let poll_interval = Duration::from_secs(self.config.poll_interval_secs.max(10));

        loop {
            let candidates = self.discover_candidate_addresses().await;
            debug!("Fetched {} candidate tokens from DexScreener profiles & boosts", candidates.len());

            for (chain, addr, desc) in candidates.into_iter().take(20) {
                if let Some(pair_data) = self.fetch_token_pair_data(&addr).await {
                    if let Some(card) = self.evaluate_and_track_candidate(chain, addr, pair_data, desc).await {
                        if let Some(ref tg) = self.telegram {
                            if let Err(e) = tg.send_alert(&card).await {
                                warn!("Failed to send multi-chain revival alert: {:?}", e);
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }

            self.prune_stale_candidates().await;
            tokio::time::sleep(poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_chains() {
        let cfg = MultiChainRevivalConfig::default();
        let scanner = MultiChainRevivalScanner::new(cfg, None);
        assert!(scanner.is_supported_chain("solana"));
        assert!(scanner.is_supported_chain("base"));
        assert!(scanner.is_supported_chain("ethereum"));
        assert!(scanner.is_supported_chain("bsc"));
        assert!(!scanner.is_supported_chain("unknown_chain"));
    }

    #[tokio::test]
    async fn test_sustained_volume_logic() {
        let mut cfg = MultiChainRevivalConfig::default();
        cfg.min_sustained_secs = 2; // Short duration for unit test
        cfg.min_age_mins = 10;
        cfg.min_5m_volume_usd = 1000.0;
        cfg.min_buy_ratio = 0.60;

        let scanner = MultiChainRevivalScanner::new(cfg, None);
        let pair = DexPairInfo {
            chain_id: Some("base".to_string()),
            pair_address: Some("0x123".to_string()),
            url: Some("https://dexscreener.com/base/0x456".to_string()),
            base_token: Some(DexTokenMeta {
                name: Some("Test Coin".to_string()),
                symbol: Some("TEST".to_string()),
            }),
            market_cap: Some(50_000.0),
            fdv: Some(50_000.0),
            volume: Some(DexVolM5 {
                m5: Some(2500.0),
                h24: Some(100_000.0),
            }),
            txns: Some(DexTxnsM5 {
                m5: Some(DexTxCount {
                    buys: Some(15),
                    sells: Some(5),
                }),
            }),
            // 2 hours old
            pair_created_at: Some(chrono::Utc::now().timestamp_millis() - 7_200_000),
        };

        // First pass: registered as candidate, no alert yet
        let first_pass = scanner.evaluate_and_track_candidate(
            "base".to_string(),
            "0x456".to_string(),
            pair,
            Some("Old dormant meme awakened".to_string()),
        ).await;
        assert!(first_pass.is_none(), "Should not alert immediately on first spike");

        // Manually adjust candidate first_detected_at to 3 seconds ago to simulate sustained volume
        {
            let mut guard = scanner.candidates.lock().await;
            if let Some(c) = guard.get_mut("base:0x456") {
                c.first_detected_at = chrono::Utc::now() - chrono::Duration::seconds(3);
            }
        }

        // Second pass: volume sustained for >= min_sustained_secs -> alert triggers!
        let pair_2 = DexPairInfo {
            chain_id: Some("base".to_string()),
            pair_address: Some("0x123".to_string()),
            url: Some("https://dexscreener.com/base/0x456".to_string()),
            base_token: Some(DexTokenMeta {
                name: Some("Test Coin".to_string()),
                symbol: Some("TEST".to_string()),
            }),
            market_cap: Some(52_000.0),
            fdv: Some(52_000.0),
            volume: Some(DexVolM5 {
                m5: Some(2800.0),
                h24: Some(102_000.0),
            }),
            txns: Some(DexTxnsM5 {
                m5: Some(DexTxCount {
                    buys: Some(20),
                    sells: Some(6),
                }),
            }),
            pair_created_at: Some(chrono::Utc::now().timestamp_millis() - 7_200_000),
        };

        let second_pass = scanner.evaluate_and_track_candidate(
            "base".to_string(),
            "0x456".to_string(),
            pair_2,
            Some("Old dormant meme awakened".to_string()),
        ).await;

        assert!(second_pass.is_some(), "Alert should trigger after sustained volume window");
        let card = second_pass.unwrap();
        assert_eq!(card.symbol, "TEST");
        assert_eq!(card.chain, "base");
        assert_eq!(card.alert_type, crate::models::alert::AlertType::MultiChainRevival);
        assert!(card.sustained_secs >= 2);
    }
}
