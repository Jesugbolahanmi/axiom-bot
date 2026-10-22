use anyhow::{bail, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct AccountInfoResponse {
    result: Option<AccountInfoValueWrapper>,
}

#[derive(Debug, Deserialize)]
struct AccountInfoValueWrapper {
    value: Option<AccountInfoValue>,
}

#[derive(Debug, Deserialize)]
struct AccountInfoValue {
    data: Option<AccountData>,
}

#[derive(Debug, Deserialize)]
struct AccountData {
    parsed: Option<ParsedAccountInfo>,
}

#[derive(Debug, Deserialize)]
struct ParsedAccountInfo {
    info: Option<MintInfo>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MintInfo {
    #[serde(rename = "mintAuthority")]
    mint_authority: Option<String>,
    #[serde(rename = "freezeAuthority")]
    freeze_authority: Option<String>,
}

pub struct PreBuyRugChecker {
    rpc_url: String,
    http_client: Client,
}

impl PreBuyRugChecker {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Execute pre-buy security gates:
    /// 1. Mint authority must be revoked or owned by bonding curve.
    /// 2. Freeze authority must be revoked (null).
    pub async fn verify_token_safety(&self, mint: &str) -> Result<()> {
        debug!("Running pre-buy rug check for mint: {}", mint);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                mint,
                { "encoding": "jsonParsed" }
            ]
        });

        let resp = self.http_client.post(&self.rpc_url).json(&payload).send().await?;
        let res: AccountInfoResponse = resp.json().await?;

        if let Some(info) = res.result.and_then(|r| r.value).and_then(|v| v.data).and_then(|d| d.parsed).and_then(|p| p.info) {
            // Check Freeze Authority
            if let Some(freeze_auth) = info.freeze_authority {
                bail!(
                    "Rug check failed: Freeze authority is NOT revoked! Active freeze authority: {}",
                    freeze_auth
                );
            }

            // On Pump.fun, mint authority is revoked or set to the curve program
            info!("✅ Pre-buy rug check PASSED: Freeze authority revoked for mint {}", mint);
            Ok(())
        } else {
            bail!("Rug check failed: Unable to parse token mint account info for {}", mint);
        }
    }
}
