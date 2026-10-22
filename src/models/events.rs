use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeCommand {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
}

/// Incoming event from PumpPortal WebSocket
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PumpPortalMessage {
    NewToken(NewTokenPayload),
    Trade(TradePayload),
    Ack(SubscriptionAck),
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionAck {
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTokenPayload {
    #[serde(rename = "txType")]
    pub tx_type: Option<String>,
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    #[serde(rename = "traderPublicKey")]
    pub trader_public_key: Option<String>,
    #[serde(rename = "solAmount")]
    pub sol_amount: Option<f64>,
    #[serde(rename = "initialBuy")]
    pub initial_buy: Option<f64>,
    #[serde(rename = "vSolInBondingCurve")]
    pub v_sol_in_bonding_curve: Option<f64>,
    #[serde(rename = "vTokensInBondingCurve")]
    pub v_tokens_in_bonding_curve: Option<f64>,
    #[serde(rename = "marketCapSol")]
    pub market_cap_sol: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradePayload {
    #[serde(rename = "txType")]
    pub tx_type: Option<String>,
    pub mint: String,
    #[serde(rename = "solAmount")]
    pub sol_amount: Option<f64>,
    #[serde(rename = "tokenAmount")]
    pub token_amount: Option<f64>,
    #[serde(rename = "vSolInBondingCurve")]
    pub v_sol_in_bonding_curve: Option<f64>,
    #[serde(rename = "vTokensInBondingCurve")]
    pub v_tokens_in_bonding_curve: Option<f64>,
    #[serde(rename = "marketCapSol")]
    pub market_cap_sol: Option<f64>,
    #[serde(rename = "traderPublicKey")]
    pub trader_public_key: Option<String>,
    pub slot: Option<u64>,
    pub signature: Option<String>,
}
