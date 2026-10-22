use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, warn};
use crate::models::alert::TopHolderItem;

#[derive(Debug, Clone)]
pub struct TokenMetrics {
    pub total_supply: f64,
    pub holder_count: u64,
    pub dev_holding_pct: f64,
    pub top_holder_pct: f64,
    pub top_holder_address: String,
    pub top_holders: Vec<TopHolderItem>,
    pub top_10_pct: f64,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TokenSupplyValue {
    #[serde(rename = "uiAmount")]
    ui_amount: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TokenSupplyResult {
    value: Option<TokenSupplyValue>,
}

#[derive(Debug, Deserialize)]
struct LargestAccountItem {
    address: String,
    #[serde(rename = "uiAmount")]
    ui_amount: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct LargestAccountsResult {
    value: Option<Vec<LargestAccountItem>>,
}

#[derive(Debug, Deserialize)]
struct DasTokenAccountItem {
    owner: Option<String>,
    amount: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DasTokenAccountsResult {
    total: Option<u64>,
    token_accounts: Option<Vec<DasTokenAccountItem>>,
}

#[derive(Clone)]
pub struct HeliusDasClient {
    rpc_url: String,
    http_client: Client,
}

impl HeliusDasClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch total token supply
    pub async fn get_token_supply(&self, mint: &str) -> Result<f64> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenSupply",
            "params": [mint]
        });

        let resp = self.http_client.post(&self.rpc_url).json(&payload).send().await?;
        let rpc_res: RpcResponse<TokenSupplyResult> = resp.json().await?;

        if let Some(err) = rpc_res.error {
            warn!("getTokenSupply error for {}: {:?}", mint, err);
            return Ok(1_000_000_000.0);
        }

        let supply = rpc_res
            .result
            .and_then(|r| r.value)
            .and_then(|v| v.ui_amount)
            .unwrap_or(1_000_000_000.0);

        Ok(supply)
    }

    /// Fetch total holders and dev wallet balance via Helius DAS getTokenAccounts
    pub async fn get_holder_summary(&self, mint: &str, dev_wallet: Option<&str>) -> Result<(u64, f64)> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenAccounts",
            "params": {
                "mint": mint,
                "page": 1,
                "limit": 100
            }
        });

        let resp = self.http_client.post(&self.rpc_url).json(&payload).send().await?;
        let rpc_res: RpcResponse<DasTokenAccountsResult> = resp.json().await?;

        if let Some(err) = rpc_res.error {
            warn!("getTokenAccounts error for {}: {:?}", mint, err);
            return Ok((1, 0.0));
        }

        let total_holders = rpc_res.result.as_ref().and_then(|r| r.total).unwrap_or(1);

        let mut dev_balance = 0.0;
        if let Some(dev) = dev_wallet {
            if let Some(accounts) = rpc_res.result.and_then(|r| r.token_accounts) {
                for acc in accounts {
                    if acc.owner.as_deref() == Some(dev) {
                        dev_balance += acc.amount.unwrap_or(0.0);
                    }
                }
            }
        }

        Ok((total_holders, dev_balance))
    }

    /// Fetch top non-curve holder percentage and list of top holders with amounts
    pub async fn get_top_holders(
        &self,
        mint: &str,
        total_supply: f64,
        dev_wallet: Option<&str>,
    ) -> Result<(f64, String, Vec<TopHolderItem>, f64)> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenLargestAccounts",
            "params": [mint]
        });

        let resp = self.http_client.post(&self.rpc_url).json(&payload).send().await?;
        let rpc_res: RpcResponse<LargestAccountsResult> = resp.json().await?;

        let accounts = rpc_res
            .result
            .and_then(|r| r.value)
            .unwrap_or_default();

        if accounts.is_empty() || total_supply <= 0.0 {
            return Ok((0.0, String::new(), Vec::new(), 0.0));
        }

        let mut user_holders: Vec<TopHolderItem> = Vec::new();
        let mut rank = 1;

        for (i, acc) in accounts.iter().enumerate() {
            let amount = acc.ui_amount.unwrap_or(0.0);
            let pct = (amount / total_supply) * 100.0;

            // On Pump.fun, account 0 holding >= 25% is the bonding curve reserve
            if i == 0 && pct >= 25.0 {
                debug!("Skipping bonding curve reserve account: {} ({:.1}%)", acc.address, pct);
                continue;
            }

            let is_dev = dev_wallet.map_or(false, |dev| dev == acc.address);

            user_holders.push(TopHolderItem {
                rank,
                address: acc.address.clone(),
                amount,
                pct,
                is_dev,
            });
            rank += 1;
        }

        let top_holder_pct = user_holders.first().map(|h| h.pct).unwrap_or(0.0);
        let top_holder_address = user_holders.first().map(|h| h.address.clone()).unwrap_or_default();
        let top_10_pct: f64 = user_holders.iter().take(10).map(|h| h.pct).sum();

        Ok((top_holder_pct, top_holder_address, user_holders, top_10_pct))
    }

    /// Combined enrichment method
    pub async fn fetch_token_metrics(&self, mint: &str, dev_wallet: Option<&str>) -> Result<TokenMetrics> {
        let total_supply = self.get_token_supply(mint).await?;
        let (holder_count, dev_balance) = self.get_holder_summary(mint, dev_wallet).await?;
        let (top_holder_pct, top_holder_address, top_holders, top_10_pct) =
            self.get_top_holders(mint, total_supply, dev_wallet).await?;

        let dev_holding_pct = if total_supply > 0.0 {
            let scaled_dev = if dev_balance > 1_000_000_000.0 {
                dev_balance / 1_000_000.0
            } else {
                dev_balance
            };
            (scaled_dev / total_supply) * 100.0
        } else {
            0.0
        };

        Ok(TokenMetrics {
            total_supply,
            holder_count,
            dev_holding_pct,
            top_holder_pct,
            top_holder_address,
            top_holders,
            top_10_pct,
        })
    }
}
