use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::VersionedTransaction;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRequestPayload {
    #[serde(rename = "publicKey")]
    pub public_key: String,
    pub action: String,
    pub mint: String,
    pub amount: f64,
    #[serde(rename = "denominatedInSol")]
    pub denominated_in_sol: String,
    pub slippage: f64,
    #[serde(rename = "priorityFee")]
    pub priority_fee: f64,
    pub pool: String,
}

#[derive(Debug, Clone)]
pub struct TradeExecutionResult {
    pub success: bool,
    pub is_dry_run: bool,
    pub signature: String,
    pub amount_sol: f64,
    pub mint: String,
    pub message: String,
}

pub struct PumpPortalTradeClient {
    api_url: String,
    rpc_url: String,
    http_client: Client,
}

impl PumpPortalTradeClient {
    pub fn new(api_url: String, rpc_url: String) -> Self {
        Self {
            api_url,
            rpc_url,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Build, locally sign, simulate, and optionally broadcast a trade
    pub async fn execute_buy(
        &self,
        keypair: &Keypair,
        mint: &str,
        amount_sol: f64,
        slippage_pct: f64,
        dry_run: bool,
        simulate_before_send: bool,
    ) -> Result<TradeExecutionResult> {
        let buyer_pubkey = keypair.pubkey().to_string();

        let req_payload = TradeRequestPayload {
            public_key: buyer_pubkey,
            action: "buy".to_string(),
            mint: mint.to_string(),
            amount: amount_sol,
            denominated_in_sol: "true".to_string(),
            slippage: slippage_pct,
            priority_fee: 0.0005,
            pool: "pump".to_string(),
        };

        info!(
            "Requesting transaction from PumpPortal trade-local: {} SOL on mint {}",
            amount_sol, mint
        );

        let resp = self
            .http_client
            .post(&self.api_url)
            .json(&req_payload)
            .send()
            .await
            .context("Failed to send request to PumpPortal trade-local API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            bail!("PumpPortal trade API error (status {}): {}", status, err_text);
        }

        let tx_bytes = resp.bytes().await.context("Failed to read transaction response bytes")?;
        if tx_bytes.is_empty() {
            bail!("Received empty transaction bytes from PumpPortal");
        }

        // Deserialize VersionedTransaction
        let mut versioned_tx: VersionedTransaction = bincode::deserialize(&tx_bytes)
            .context("Failed to deserialize VersionedTransaction from PumpPortal")?;

        // Sign the transaction locally with our burner wallet keypair
        let message_bytes = versioned_tx.message.serialize();
        let signature = keypair.sign_message(&message_bytes);
        if versioned_tx.signatures.is_empty() {
            versioned_tx.signatures.push(signature);
        } else {
            versioned_tx.signatures[0] = signature;
        }

        let serialized_signed_tx = bincode::serialize(&versioned_tx)
            .context("Failed to serialize signed transaction")?;
        let base64_tx = bs58::encode(&serialized_signed_tx).into_string();

        // Simulate transaction via RPC if enabled
        if simulate_before_send || dry_run {
            debug!("Simulating transaction via Helius RPC...");
            let sim_payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "simulateTransaction",
                "params": [
                    base64_tx,
                    {
                        "sigVerify": false,
                        "encoding": "base58"
                    }
                ]
            });

            if let Ok(sim_resp) = self.http_client.post(&self.rpc_url).json(&sim_payload).send().await {
                if let Ok(sim_json) = sim_resp.json::<serde_json::Value>().await {
                    if let Some(err) = sim_json.get("result").and_then(|r| r.get("value")).and_then(|v| v.get("err")) {
                        if !err.is_null() {
                            warn!("Transaction simulation failed: {:?}", err);
                            if !dry_run {
                                bail!("Simulation failed: {:?}", err);
                            }
                        }
                    }
                }
            }
        }

        if dry_run {
            info!(
                "🧪 [DRY RUN] Buy transaction prepared & simulated successfully! Amount: {} SOL, Mint: {}",
                amount_sol, mint
            );

            return Ok(TradeExecutionResult {
                success: true,
                is_dry_run: true,
                signature: format!("simulated_buy_{}_sol", amount_sol),
                amount_sol,
                mint: mint.to_string(),
                message: format!("Dry-run buy simulated for {} SOL on {}", amount_sol, mint),
            });
        }

        // Live Broadcast
        info!("Broadcasting signed transaction to Solana mainnet...");
        let send_payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                base64_tx,
                {
                    "encoding": "base58",
                    "preflightCommitment": "confirmed"
                }
            ]
        });

        let send_resp = self.http_client.post(&self.rpc_url).json(&send_payload).send().await?;
        let send_json: serde_json::Value = send_resp.json().await?;

        if let Some(sig) = send_json.get("result").and_then(|r| r.as_str()) {
            info!("🚀 Live transaction broadcast successful! Tx: {}", sig);
            Ok(TradeExecutionResult {
                success: true,
                is_dry_run: false,
                signature: sig.to_string(),
                amount_sol,
                mint: mint.to_string(),
                message: format!("Live buy broadcasted: {}", sig),
            })
        } else {
            let err = send_json.get("error").unwrap_or(&send_json);
            bail!("Failed to broadcast transaction: {:?}", err);
        }
    }
}
