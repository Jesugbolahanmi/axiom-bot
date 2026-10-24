pub mod multi_chain_revival;
pub mod dex_screener;
pub mod helius_das;
pub mod sniper_bundler;

use std::sync::Arc;
use anyhow::Result;
use tracing::{debug, info, warn};

pub use dex_screener::DexScreenerClient;
pub use helius_das::HeliusDasClient;
pub use sniper_bundler::SniperBundlerAnalyzer;
pub use multi_chain_revival::MultiChainRevivalScanner;
use crate::models::token::TokenState;

#[derive(Clone)]
pub struct EnrichmentService {
    das_client: Arc<HeliusDasClient>,
    sniper_analyzer: Arc<SniperBundlerAnalyzer>,
    dex_client: Arc<DexScreenerClient>,
}

impl EnrichmentService {
    pub fn new(
        rpc_url: String,
        sniper_block_window: u64,
        trace_funding_cluster: bool,
    ) -> Self {
        let das_client = Arc::new(HeliusDasClient::new(rpc_url.clone()));
        let sniper_analyzer = Arc::new(SniperBundlerAnalyzer::new(
            rpc_url,
            sniper_block_window,
            trace_funding_cluster,
        ));
        let dex_client = Arc::new(DexScreenerClient::new());

        Self {
            das_client,
            sniper_analyzer,
            dex_client,
        }
    }

    /// Perform full on-chain and market data enrichment for a candidate token
    pub async fn enrich_token(&self, token: &mut TokenState) -> Result<()> {
        let mint = &token.mint;
        let dev = token.dev_wallet.as_deref();

        // 1. Fetch live market cap, volume, and liquidity from DexScreener
        if let Ok(Some(dex_metrics)) = self.dex_client.fetch_metrics(mint).await {
            if dex_metrics.market_cap_usd > 0.0 {
                token.market_cap_usd = dex_metrics.market_cap_usd;
                token.liquidity_usd = dex_metrics.liquidity_usd;
                token.total_volume_usd = dex_metrics.total_volume_usd;
                token.rolling_5m_volume_usd = dex_metrics.volume_5m_usd;
            }
            if let Some(ref sym) = dex_metrics.base_token_symbol {
                if !sym.is_empty() && (token.symbol == "UNKNOWN" || token.symbol.is_empty()) {
                    token.symbol = sym.clone();
                }
            }
            if let Some(ref name) = dex_metrics.base_token_name {
                if !name.is_empty() && (token.name == "Unknown Token" || token.name.is_empty()) {
                    token.name = name.clone();
                }
            }
        }
        token.last_updated = chrono::Utc::now();

        // If market cap is below $4k and no recent 5m volume spike, skip expensive Helius RPC calls
        if token.market_cap_usd < 4000.0 && token.rolling_5m_volume_usd < 500.0 && token.holder_count < 10 {
            debug!("Token {} MCap (${:.0}) below threshold, skipping DAS/sniper analysis", mint, token.market_cap_usd);
            return Ok(());
        }

        debug!("Enriching token {} with Helius on-chain data...", mint);

        // 2. Fetch holder count, dev holding %, and top non-curve holder %
        match self.das_client.fetch_token_metrics(mint, dev).await {
            Ok(metrics) => {
                token.holder_count = metrics.holder_count;
                token.dev_holding_pct = metrics.dev_holding_pct;
                token.top_holder_pct = metrics.top_holder_pct;
                token.top_holders = metrics.top_holders;
                token.top_10_pct = metrics.top_10_pct;

                info!(
                    "Enriched metrics for ${} ({}): MCap=${:.0}, Vol=${:.0}, Holders={}, Dev={:.1}%, TopHolder={:.1}%",
                    token.symbol, mint, token.market_cap_usd, token.total_volume_usd, token.holder_count, token.dev_holding_pct, token.top_holder_pct
                );

                // 3. Run early-block sniper/bundler heuristic analysis
                match self.sniper_analyzer.analyze(mint, dev, metrics.total_supply).await {
                    Ok(sniper_res) => {
                        token.sniper_bundler_pct = sniper_res.combined_share_pct;
                        token.clustered_wallets_count = sniper_res.bundler_wallet_count;
                        debug!(
                            "Sniper/Bundler analysis for {}: {:.1}% - {}",
                            mint, sniper_res.combined_share_pct, sniper_res.heuristic_summary
                        );
                    }
                    Err(e) => {
                        warn!("Sniper/bundler analysis error for {}: {:?}", mint, e);
                    }
                }
            }
            Err(e) => {
                warn!("Helius DAS enrichment error for {}: {:?}", mint, e);
            }
        }

        Ok(())
    }
}
