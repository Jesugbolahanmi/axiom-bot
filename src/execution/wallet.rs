use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::json;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use tracing::info;

pub struct BurnerWallet {
    keypair: Keypair,
}

impl BurnerWallet {
    /// Load burner wallet from private key environment variable
    pub fn load_from_env(env_var: &str) -> Result<Self> {
        let raw_key = std::env::var(env_var)
            .with_context(|| format!("Environment variable '{}' is not set", env_var))?;

        let raw_trimmed = raw_key.trim();
        if raw_trimmed.is_empty() {
            bail!("Burner wallet private key in '{}' is empty", env_var);
        }

        let keypair = if raw_trimmed.starts_with('[') && raw_trimmed.ends_with(']') {
            // JSON byte array format: [1, 2, 3, ...]
            let bytes: Vec<u8> = serde_json::from_str(raw_trimmed)
                .context("Failed to parse JSON byte array private key")?;
            Keypair::from_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("Invalid keypair bytes: {:?}", e))?
        } else {
            // Base58 encoded format
            let bytes = bs58::decode(raw_trimmed)
                .into_vec()
                .context("Failed to decode Base58 private key")?;
            Keypair::from_bytes(&bytes)
                .map_err(|e| anyhow::anyhow!("Invalid Base58 keypair bytes: {:?}", e))?
        };

        info!("Loaded dedicated burner wallet pubkey: {}", keypair.pubkey());
        Ok(Self { keypair })
    }

    pub fn pubkey_base58(&self) -> String {
        self.keypair.pubkey().to_string()
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Query current SOL balance via RPC
    pub async fn fetch_balance_sol(&self, rpc_url: &str) -> Result<f64> {
        let client = Client::new();
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [
                self.pubkey_base58(),
                { "commitment": "confirmed" }
            ]
        });

        let resp = client.post(rpc_url).json(&payload).send().await?;
        let res_json: serde_json::Value = resp.json().await?;

        if let Some(lamports) = res_json.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_f64()) {
            Ok(lamports / 1_000_000_000.0)
        } else {
            bail!("Failed to parse balance response: {:?}", res_json);
        }
    }
}
