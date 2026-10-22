pub mod pumpportal_trade;
pub mod rug_check;
pub mod wallet;

use std::sync::Arc;
use anyhow::{bail, Result};
use tracing::{info, warn};

pub use pumpportal_trade::{PumpPortalTradeClient, TradeExecutionResult};
pub use rug_check::PreBuyRugChecker;
pub use wallet::BurnerWallet;
use crate::config::AppConfig;
use crate::safety::SafetyGuard;

pub struct ExecutionEngine {
    wallet: Option<BurnerWallet>,
    trade_client: PumpPortalTradeClient,
    rug_checker: PreBuyRugChecker,
    safety_guard: Arc<SafetyGuard>,
    dry_run: bool,
    max_slippage_pct: f64,
    simulate_before_send: bool,
    rpc_url: String,
}

impl ExecutionEngine {
    pub fn new(config: &AppConfig, safety_guard: Arc<SafetyGuard>) -> Self {
        let wallet = BurnerWallet::load_from_env("SOLANA_PRIVATE_KEY").ok();
        let rpc_url = config.sources.helius_rpc_url.clone().unwrap_or_default();

        let trade_client = PumpPortalTradeClient::new(
            config.sources.pumpportal_trade_api.clone(),
            rpc_url.clone(),
        );
        let rug_checker = PreBuyRugChecker::new(rpc_url.clone());

        Self {
            wallet,
            trade_client,
            rug_checker,
            safety_guard,
            dry_run: config.bot.dry_run,
            max_slippage_pct: config.safety.max_slippage_pct,
            simulate_before_send: config.safety.simulate_before_send,
            rpc_url,
        }
    }

    pub fn wallet(&self) -> Option<&BurnerWallet> {
        self.wallet.as_ref()
    }

    pub fn safety_guard(&self) -> Arc<SafetyGuard> {
        self.safety_guard.clone()
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Query current balance of the burner wallet
    pub async fn get_burner_balance_sol(&self) -> Result<f64> {
        if let Some(ref wallet) = self.wallet {
            wallet.fetch_balance_sol(&self.rpc_url).await
        } else {
            bail!("No burner wallet configured in environment (SOLANA_PRIVATE_KEY)");
        }
    }

    /// Unified Manual or Automated Buy Execution:
    /// 1. Hard spending and safety caps check.
    /// 2. Pre-buy rug check (freeze authority revoked).
    /// 3. Request trade transaction from PumpPortal.
    /// 4. Sign locally with burner keypair.
    /// 5. Simulate via RPC.
    /// 6. If dry_run is false, broadcast to Solana mainnet; otherwise log and return dry-run receipt.
    pub async fn execute_buy(&self, mint: &str, amount_sol: f64) -> Result<TradeExecutionResult> {
        // 1. Safety validation (Kill switch, caps, position limits)
        self.safety_guard.validate_trade(mint, amount_sol)?;

        let wallet = match &self.wallet {
            Some(w) => w,
            None => {
                bail!("Cannot execute buy: Burner wallet private key is not configured");
            }
        };

        // 2. Pre-buy rug check (ensure freeze authority is revoked)
        if !self.rpc_url.is_empty() {
            if let Err(e) = self.rug_checker.verify_token_safety(mint).await {
                warn!("Rug check blocked trade on mint {}: {:?}", mint, e);
                bail!("Pre-buy security check failed: {}", e);
            }
        }

        // 3. Execute swap via PumpPortal trade client
        let result = self
            .trade_client
            .execute_buy(
                wallet.keypair(),
                mint,
                amount_sol,
                self.max_slippage_pct,
                self.dry_run,
                self.simulate_before_send,
            )
            .await?;

        // 4. Record spend in safety tracker
        self.safety_guard
            .record_spend(&result.signature, mint, amount_sol);

        info!(
            "Buy order finished: success={}, dry_run={}, tx={}",
            result.success, result.is_dry_run, result.signature
        );

        Ok(result)
    }
}
