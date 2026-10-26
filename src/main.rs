use axiom_bot::alerts::{AlertDeduplicator, TelegramBotService};
use axiom_bot::config::AppConfig;
use axiom_bot::filter::FilterEngine;
use axiom_bot::listeners::pumpportal::PumpPortalListener;
use axiom_bot::listeners::LaunchpadListener;
use axiom_bot::models::alert::AlertCard;
use axiom_bot::state::TokenStore;

use std::sync::Arc;
use std::time::Duration;
use anyhow::{Context, Result};
use tokio::time::sleep;
use tracing::{error, info, warn};


#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Solana New-Coin Filter & Alert Bot (Phase 1)");

    // Load configuration
    let config = AppConfig::load_from_file_and_env("config.toml")
        .context("Failed to load application configuration")?;

    info!(
        "Configuration loaded: Age [{}m - {}m], MCap [${} - ${}], Vol ratio >= {:.1}x, Liq >= {:.0}%",
        config.filters.min_age_mins,
        config.filters.max_age_mins,
        config.filters.min_mcap_usd,
        config.filters.max_mcap_usd,
        config.filters.min_total_volume_mult,
        config.filters.min_liquidity_ratio * 100.0
    );

    // Initial state store (defaulting SOL price to $150; will update in Phase 2 via Pyth / CoinGecko / Birdeye)
    let initial_sol_price = 150.0;
    let store = TokenStore::new(initial_sol_price);

    // Filter engine and deduplicator
    let filter_engine = Arc::new(FilterEngine::new(config.filters.clone(), config.revival_filters.clone()));
    let deduplicator = Arc::new(AlertDeduplicator::new(config.telegram.alert_cooldown_mins));

    // Safety Guard & Execution Engine
    let safety_guard = Arc::new(axiom_bot::safety::SafetyGuard::new(config.safety.clone()));
    let execution_engine = Arc::new(axiom_bot::execution::ExecutionEngine::new(
        &config,
        safety_guard.clone(),
    ));

    info!(
        "Execution Engine initialized (Dry Run: {}, Max/Trade: {:.2} SOL, Max Daily: {:.2} SOL)",
        config.bot.dry_run, config.safety.max_sol_per_trade, config.safety.max_daily_spend_sol
    );

    // Optional Telegram service
    let telegram_service = if !config.telegram.bot_token.is_empty() {
        info!("Telegram Bot Token detected. Setting up Telegram alerting & execution...");
        let tg = Arc::new(TelegramBotService::new(
            config.telegram.bot_token.clone(),
            config.telegram.allowed_user_id,
            Some(execution_engine.clone()),
        ));

        // Spawn telegram bot callback/command listener
        let tg_clone = tg.clone();
        tokio::spawn(async move {
            tg_clone.start_event_handler().await;
        });

        Some(tg)
    } else {
        warn!("⚠️ No TELEGRAM_BOT_TOKEN set in environment. Alerts will only be logged to console.");
        None
    };

    // Optional Enrichment service (Helius DAS & Sniper/Bundler heuristics)
    let enrichment_service = if let Some(ref rpc_url) = config.sources.helius_rpc_url {
        info!("Helius RPC detected. Activating On-Chain Deep Enrichment & Sniper/Bundler heuristics...");
        Some(Arc::new(axiom_bot::enrichment::EnrichmentService::new(
            rpc_url.clone(),
            config.heuristics.sniper_block_window,
            config.heuristics.trace_funding_cluster,
        )))
    } else {
        warn!("⚠️ No HELIUS_RPC_URL set. On-chain deep enrichment and sniper analysis will be skipped.");
        None
    };

    // 1. Launch PumpPortal WebSocket Listener
    let ws_listener = PumpPortalListener::new(
        config.sources.pumpportal_ws_url.clone(),
        store.clone(),
        config.filters.min_lore_length,
    );
    tokio::spawn(async move {
        if let Err(e) = ws_listener.run().await {
            error!("PumpPortal WebSocket listener terminated with error: {:?}", e);
        }
    });

    // 2. Periodic Filter Evaluation & Pruning Worker
    let poll_interval = Duration::from_secs(config.bot.poll_interval_secs);
    let store_worker = store.clone();
    let filter_worker = filter_engine.clone();
    let dedup_worker = deduplicator.clone();
    let tg_worker = telegram_service.clone();
    let enrichment_worker = enrichment_service.clone();
    let _min_age = config.filters.min_age_mins as f64;
    let max_enrich_age_mins = config.filters.max_age_mins; // .max(config.revival_filters.max_age_mins); [PAUSED REVIVAL]

    tokio::spawn(async move {
        info!("Started filter evaluation worker (interval: {:?})", poll_interval);
        let mut loop_count: u64 = 0;

        loop {
            sleep(poll_interval).await;
            loop_count += 1;

            let active_tokens = store_worker.get_active_tokens();
            for mut token in active_tokens {
                let age = token.age_mins();
                let elapsed_since_enrich = (chrono::Utc::now() - token.last_updated).num_seconds();

                // Enrich tokens that are >= 1.0m old and haven't been checked in the last 25 seconds
                if age >= 1.0 && age <= max_enrich_age_mins as f64 && elapsed_since_enrich >= 25 {
                    if let Some(ref enrichment) = enrichment_worker {
                        let _ = enrichment.enrich_token(&mut token).await;
                        store_worker.update_token_enrichment(&token);
                    }
                }

                // 1. Evaluate standard Early Coin filter
                let result = filter_worker.evaluate(&token);
                if result.passed {
                    if dedup_worker.should_alert(&token.mint) {
                        let card = AlertCard::from(&token);
                        info!(
                            "🔥 QUALIFYING TOKEN FOUND! ${} ({}) - MCap: ${:.0}, Age: {:.1}m, Dev: {:.1}%, TopHolder: {:.1}%, Snipers: {:.1}%",
                            card.symbol, card.name, card.market_cap_usd, card.age_mins, card.dev_holding_pct, card.top_holder_pct, card.sniper_bundler_pct
                        );

                        if let Some(ref tg) = tg_worker {
                            if let Err(e) = tg.send_alert(&card).await {
                                error!("Failed to send alert via Telegram: {:?}", e);
                            }
                        }

                        dedup_worker.record_alert(&token.mint);
                        store_worker.mark_alert_sent(&token.mint);
                    }
                } else {
                    // [PAUSED] 2. Evaluate Token Revival filter (dormant CTO with sudden surge)
                    // let revival_result = filter_worker.evaluate_revival(&token);
                    // if revival_result.passed {
                    //     let revival_key = format!("revival:{}", token.mint);
                    //     if dedup_worker.should_alert(&revival_key) {
                    //         let card = AlertCard::new_revival(&token);
                    //         info!(
                    //             "⚡ TOKEN REVIVAL DETECTED! ${} ({}) - Reclaim: ${:.0} -> ${:.0} (+{:.1}%), 5m Vol: ${:.0}, Dev: {:.1}%",
                    //             card.symbol, card.name, card.bottom_market_cap_usd, card.market_cap_usd, card.surge_pct_from_bottom, card.rolling_5m_volume_usd, card.dev_holding_pct
                    //         );
                    //
                    //         if let Some(ref tg) = tg_worker {
                    //             if let Err(e) = tg.send_alert(&card).await {
                    //                 error!("Failed to send revival alert via Telegram: {:?}", e);
                    //             }
                    //         }
                    //
                    //         dedup_worker.record_alert(&revival_key);
                    //         store_worker.mark_alert_sent(&token.mint);
                    //     }
                    // } else
                    if token.holder_count >= 10 || token.market_cap_usd >= 7500.0 {
                        info!(
                            "📊 Candidate ${} ({}) maturing: MCap=${:.0}, Vol=${:.0}, Holders={} (Needs $10k+, 50+ holders) | Pending: {}",
                            token.symbol, token.mint, token.market_cap_usd, token.total_volume_usd, token.holder_count,
                            result.rejection_reason.as_deref().unwrap_or("Filters not met")
                        );
                    }
                }
            }

            // Prune expired tokens every 12 loops (~1 minute)
            if loop_count % 12 == 0 {
                store_worker.prune_expired_tokens(config.filters.max_age_mins); // [PAUSED REVIVAL]
                dedup_worker.prune();
            }
        }
    });

    // [PAUSED] 3. Multi-Chain Revival Scanner Worker (Solana, Base, Ethereum, BSC)
    // if config.multi_chain_revival.enabled {
    //     let multi_chain_scanner = Arc::new(axiom_bot::enrichment::MultiChainRevivalScanner::new(
    //         config.multi_chain_revival.clone(),
    //         telegram_service.clone(),
    //     ));
    //
    //     let scanner_worker = multi_chain_scanner.clone();
    //     tokio::spawn(async move {
    //         scanner_worker.run().await;
    //     });
    //     info!("Spawned Multi-Chain Revival Scanner daemon across chains {:?}", config.multi_chain_revival.chains);
    // }

    info!("Bot is fully operational. Press Ctrl+C to terminate.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down bot gracefully...");

    Ok(())
}
