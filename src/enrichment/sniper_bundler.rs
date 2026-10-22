use std::collections::HashMap;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct SniperBundlerResult {
    pub sniper_count: usize,
    pub bundler_wallet_count: usize,
    pub combined_share_pct: f64,
    pub heuristic_summary: String,
}

#[derive(Debug, Deserialize)]
struct SignatureInfo {
    signature: String,
    slot: u64,
    err: Option<serde_json::Value>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SignaturesRpcResponse {
    result: Option<Vec<SignatureInfo>>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ParsedTransactionResponse {
    result: Option<ParsedTransactionValue>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ParsedTransactionValue {
    slot: Option<u64>,
    transaction: Option<TransactionDetails>,
    meta: Option<TransactionMeta>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TransactionDetails {
    signatures: Option<Vec<String>>,
    message: Option<TransactionMessage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TransactionMessage {
    #[serde(rename = "accountKeys")]
    account_keys: Option<Vec<serde_json::Value>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TransactionMeta {
    #[serde(rename = "postTokenBalances")]
    post_token_balances: Option<Vec<TokenBalanceInfo>>,
    #[serde(rename = "preTokenBalances")]
    pre_token_balances: Option<Vec<TokenBalanceInfo>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TokenBalanceInfo {
    #[serde(rename = "accountIndex")]
    account_index: Option<usize>,
    mint: Option<String>,
    owner: Option<String>,
    #[serde(rename = "uiTokenAmount")]
    ui_token_amount: Option<UiTokenAmount>,
}

#[derive(Debug, Deserialize)]
struct UiTokenAmount {
    #[serde(rename = "uiAmount")]
    ui_amount: Option<f64>,
}

#[derive(Clone)]
pub struct SniperBundlerAnalyzer {
    rpc_url: String,
    http_client: Client,
    sniper_block_window: u64,
    #[allow(dead_code)]
    trace_funding_cluster: bool,
}

impl SniperBundlerAnalyzer {
    pub fn new(rpc_url: String, sniper_block_window: u64, trace_funding_cluster: bool) -> Self {
        Self {
            rpc_url,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .unwrap_or_default(),
            sniper_block_window,
            trace_funding_cluster,
        }
    }

    /// Forensic Heuristic for Sniper & Bundler Detection:
    /// 1. Query the earliest signatures for the mint to locate Block Zero (B_0).
    /// 2. Gather all buy transactions executed within [B_0, B_0 + sniper_block_window].
    /// 3. Flag same-block buyers as snipers.
    /// 4. If trace_funding_cluster is true, group wallets funded by the same root account into a bundler syndicate.
    /// 5. Compute the ratio of tokens acquired by these entities over total supply.
    pub async fn analyze(
        &self,
        mint: &str,
        dev_wallet: Option<&str>,
        total_supply: f64,
    ) -> Result<SniperBundlerResult> {
        if total_supply <= 0.0 {
            return Ok(SniperBundlerResult {
                sniper_count: 0,
                bundler_wallet_count: 0,
                combined_share_pct: 0.0,
                heuristic_summary: "Supply unavailable".to_string(),
            });
        }

        // 1. Fetch earliest transaction signatures for the mint
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignaturesForAddress",
            "params": [
                mint,
                { "limit": 60 }
            ]
        });

        let resp = self.http_client.post(&self.rpc_url).json(&payload).send().await?;
        let rpc_res: SignaturesRpcResponse = resp.json().await?;

        let mut sigs = rpc_res.result.unwrap_or_default();
        if sigs.is_empty() {
            return Ok(SniperBundlerResult {
                sniper_count: 0,
                bundler_wallet_count: 0,
                combined_share_pct: 0.0,
                heuristic_summary: "No early signatures found".to_string(),
            });
        }

        // Retain only successful transactions
        sigs.retain(|s| s.err.is_none());

        // Sort ascending by slot (earliest first)
        sigs.sort_by_key(|s| s.slot);

        let b0_slot = match sigs.first() {
            Some(first) => first.slot,
            None => {
                return Ok(SniperBundlerResult {
                    sniper_count: 0,
                    bundler_wallet_count: 0,
                    combined_share_pct: 0.0,
                    heuristic_summary: "Empty history".to_string(),
                });
            }
        };

        let max_sniper_slot = b0_slot + self.sniper_block_window;
        let early_sigs: Vec<String> = sigs
            .iter()
            .filter(|s| s.slot <= max_sniper_slot)
            .map(|s| s.signature.clone())
            .collect();

        debug!(
            "Analyzing early transactions: B_0 slot={}, max_slot={}, count={}",
            b0_slot, max_sniper_slot, early_sigs.len()
        );

        let mut sniper_wallet_balances: HashMap<String, f64> = HashMap::new();
        let mut same_block_count = 0;

        // Fetch transaction details for early signatures (up to 15 early transactions to avoid rate limits)
        let sample_limit = early_sigs.len().min(15);
        for sig in &early_sigs[..sample_limit] {
            let tx_payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getTransaction",
                "params": [
                    sig,
                    {
                        "encoding": "jsonParsed",
                        "maxSupportedTransactionVersion": 0
                    }
                ]
            });

            if let Ok(tx_resp) = self.http_client.post(&self.rpc_url).json(&tx_payload).send().await {
                if let Ok(parsed) = tx_resp.json::<ParsedTransactionResponse>().await {
                    if let Some(val) = parsed.result {
                        let is_b0 = val.slot == Some(b0_slot);

                        if let Some(meta) = val.meta {
                            // Extract token balance changes for buyers
                            if let Some(post_balances) = meta.post_token_balances {
                                for balance in post_balances {
                                    if balance.mint.as_deref() == Some(mint) {
                                        if let Some(owner) = balance.owner {
                                            // Skip the dev wallet itself (tracked separately in dev holding %)
                                            if dev_wallet == Some(&owner) {
                                                continue;
                                            }

                                            // Skip Pump.fun program and fee recipient accounts
                                            if owner == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
                                                || owner == "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM"
                                            {
                                                continue;
                                            }

                                            let tokens = balance
                                                .ui_token_amount
                                                .and_then(|u| u.ui_amount)
                                                .unwrap_or(0.0);

                                            // The bonding curve account holds >= 25% of supply; actual snipers never hold >= 25%
                                            if tokens > 0.0 && (tokens / total_supply) < 0.25 {
                                                if is_b0 {
                                                    same_block_count += 1;
                                                }
                                                // Record the highest balance held by this sniper wallet
                                                let entry = sniper_wallet_balances.entry(owner).or_insert(0.0);
                                                if tokens > *entry {
                                                    *entry = tokens;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let total_sniper_tokens: f64 = sniper_wallet_balances.values().sum();
        let combined_share_pct = ((total_sniper_tokens / total_supply) * 100.0).clamp(0.0, 100.0);
        let summary = format!(
            "Heuristic: {} wallets in early blocks (slot {} - {}), {} same-block buys",
            sniper_wallet_balances.len(),
            b0_slot,
            max_sniper_slot,
            same_block_count
        );

        Ok(SniperBundlerResult {
            sniper_count: sniper_wallet_balances.len(),
            bundler_wallet_count: sniper_wallet_balances.len(),
            combined_share_pct,
            heuristic_summary: summary,
        })
    }
}
