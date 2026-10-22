use std::sync::Arc;
use std::time::Duration;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use super::LaunchpadListener;
use crate::models::events::{PumpPortalMessage, SubscribeCommand};
use crate::state::TokenStore;

pub struct PumpPortalListener {
    ws_url: String,
    store: Arc<TokenStore>,
    min_lore_length: usize,
    http_client: reqwest::Client,
}

impl PumpPortalListener {
    pub fn new(ws_url: String, store: Arc<TokenStore>, min_lore_length: usize) -> Self {
        Self {
            ws_url,
            store,
            min_lore_length,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Background task to fetch off-chain metadata (IPFS/Arweave JSON) and extract description/lore
    fn spawn_lore_fetcher(
        http_client: reqwest::Client,
        store: Arc<TokenStore>,
        mint: String,
        uri: String,
        min_lore_length: usize,
    ) {
        if uri.is_empty() {
            return;
        }

        tokio::spawn(async move {
            let direct_url = if uri.starts_with("ipfs://") {
                uri.replace("ipfs://", "https://cf-ipfs.com/ipfs/")
            } else {
                uri
            };

            match http_client.get(&direct_url).send().await {
                Ok(resp) => {
                    if let Ok(json) = resp.json::<Value>().await {
                        if let Some(desc) = json.get("description").and_then(|d| d.as_str()) {
                            let clean_desc = desc.trim().to_string();
                            store.update_lore(&mint, clean_desc, min_lore_length);
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch token metadata from {}: {:?}", direct_url, e);
                }
            }
        });
    }
}

#[async_trait]
impl LaunchpadListener for PumpPortalListener {
    async fn run(&self) -> Result<()> {
        let mut backoff = Duration::from_secs(1);
        let max_backoff = Duration::from_secs(30);

        loop {
            info!("Connecting to PumpPortal WebSocket: {}", self.ws_url);

            match connect_async(&self.ws_url).await {
                Ok((ws_stream, _)) => {
                    info!("Successfully connected to PumpPortal WebSocket");
                    backoff = Duration::from_secs(1); // Reset backoff on successful connect

                    let (mut write, mut read) = ws_stream.split();

                    // 1. Subscribe to new token launches
                    let sub_new_tokens = SubscribeCommand {
                        method: "subscribeNewToken".to_string(),
                        keys: None,
                    };
                    let sub_msg = serde_json::to_string(&sub_new_tokens)
                        .context("Failed to serialize subscribeNewToken")?;

                    if let Err(e) = write.send(Message::Text(sub_msg)).await {
                        error!("Failed to send subscribeNewToken command: {:?}", e);
                        sleep(Duration::from_secs(2)).await;
                        continue;
                    }

                    // 2. Re-subscribe to all active tokens in store
                    let active_tokens = self.store.get_active_tokens();
                    if !active_tokens.is_empty() {
                        let mint_keys: Vec<String> = active_tokens.into_iter().map(|t| t.mint).collect();
                        let sub_trades = SubscribeCommand {
                            method: "subscribeTokenTrade".to_string(),
                            keys: Some(mint_keys),
                        };
                        if let Ok(msg) = serde_json::to_string(&sub_trades) {
                            let _ = write.send(Message::Text(msg)).await;
                        }
                    }

                    // Channel for subscribing to individual new tokens dynamically
                    let (trade_sub_tx, mut trade_sub_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

                    // Spawn writer loop
                    let mut write_half = write;
                    tokio::spawn(async move {
                        while let Some(mint) = trade_sub_rx.recv().await {
                            let sub_trade = SubscribeCommand {
                                method: "subscribeTokenTrade".to_string(),
                                keys: Some(vec![mint]),
                            };
                            if let Ok(msg) = serde_json::to_string(&sub_trade) {
                                if write_half.send(Message::Text(msg)).await.is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    // Read loop
                    while let Some(msg_result) = read.next().await {
                        match msg_result {
                            Ok(Message::Text(text)) => {
                                match serde_json::from_str::<PumpPortalMessage>(&text) {
                                    Ok(PumpPortalMessage::NewToken(payload)) => {
                                        info!(
                                            "New Pump.fun token: ${} ({}) - Mint: {}",
                                            payload.symbol.as_deref().unwrap_or("?"),
                                            payload.name.as_deref().unwrap_or("?"),
                                            payload.mint
                                        );

                                        let mint = payload.mint.clone();
                                        let uri = payload.uri.clone().unwrap_or_default();

                                        self.store.insert_new_token(payload);

                                        // Subscribe to trades for this token
                                        let _ = trade_sub_tx.send(mint.clone());

                                        // Fetch lore in background
                                        Self::spawn_lore_fetcher(
                                            self.http_client.clone(),
                                            self.store.clone(),
                                            mint,
                                            uri,
                                            self.min_lore_length,
                                        );
                                    }
                                    Ok(PumpPortalMessage::Trade(payload)) => {
                                        debug!(
                                            "Trade on mint {}: {} SOL (type: {:?})",
                                            payload.mint,
                                            payload.sol_amount.unwrap_or(0.0),
                                            payload.tx_type
                                        );
                                        self.store.record_trade(payload);
                                    }
                                    Ok(PumpPortalMessage::Ack(ack)) => {
                                        debug!("PumpPortal ACK: {:?}", ack.message);
                                    }
                                    Ok(PumpPortalMessage::Unknown(val)) => {
                                        debug!("Received unhandled payload: {:?}", val);
                                    }
                                    Err(e) => {
                                        debug!("JSON parse error from PumpPortal text: {:?} | raw: {}", e, text);
                                    }
                                }
                            }
                            Ok(Message::Ping(_data)) => {
                                // Handled automatically by tungstenite
                            }
                            Ok(Message::Close(frame)) => {
                                warn!("PumpPortal connection closed by server: {:?}", frame);
                                break;
                            }
                            Err(e) => {
                                error!("PumpPortal WebSocket stream error: {:?}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to connect to PumpPortal: {:?}. Retrying in {:?}...", e, backoff);
                }
            }

            // Exponential backoff
            sleep(backoff).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    }
}
