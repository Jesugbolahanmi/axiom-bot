use axiom_bot::config::thresholds::FilterThresholds;
use axiom_bot::filter::FilterEngine;
use axiom_bot::models::alert::AlertCard;
use axiom_bot::models::token::TokenState;
use axiom_bot::state::rolling_window::TradeRollingWindow;
use axiom_bot::alerts::deduplicator::AlertDeduplicator;
use axiom_bot::alerts::formatter::format_alert_html;
use chrono::{Duration, Utc};

fn create_sample_token(age_mins: i64, mcap_usd: f64, total_vol_usd: f64, vol_5m_usd: f64, liq_usd: f64) -> TokenState {
    let now = Utc::now();
    let created_at = now - Duration::minutes(age_mins);

    TokenState {
        mint: "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string(),
        name: "Pepe Early".to_string(),
        symbol: "PEPE".to_string(),
        uri: "https://example.com/meta.json".to_string(),
        description: Some("The official test token with lore".to_string()),
        dev_wallet: Some("Dev11111111111111111111111111111111111111111".to_string()),
        created_at,
        last_updated: now,
        market_cap_sol: mcap_usd / 150.0,
        market_cap_usd: mcap_usd,
        lowest_market_cap_usd: mcap_usd,
        liquidity_sol: liq_usd / 150.0,
        liquidity_usd: liq_usd,
        total_volume_sol: total_vol_usd / 150.0,
        total_volume_usd: total_vol_usd,
        rolling_5m_volume_sol: vol_5m_usd / 150.0,
        rolling_5m_volume_usd: vol_5m_usd,
        holder_count: 65,
        dev_holding_pct: 5.0,
        top_holder_pct: 8.0,
        sniper_bundler_pct: 4.5,
        has_lore: true,
        alert_sent_at: None,
        top_holders: Vec::new(),
        top_10_pct: 12.0,
        clustered_wallets_count: 0,
    }
}

#[test]
fn test_qualifying_token_passes_filter() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    // Age 15m, MCap $20k, Volume $25k (1.25x), 5m Vol $3k (15%), Liquidity $9k (45%), Holders 65
    let token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    let result = engine.evaluate(&token);

    assert!(result.passed, "Qualifying token should pass filter: {:?}", result.rejection_reason);
}

#[test]
fn test_token_too_young_is_rejected() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    // Age 2m (min is 5m)
    let token = create_sample_token(2, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    let result = engine.evaluate(&token);

    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("below minimum"));
}

#[test]
fn test_token_mcap_out_of_bounds() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    // MCap $5,000 (below $10,000)
    let low_mcap = create_sample_token(15, 5_000.0, 10_000.0, 1_000.0, 2_500.0);
    let result_low = engine.evaluate(&low_mcap);
    assert!(!result_low.passed);

    // MCap $40,000 (above $35,000)
    let high_mcap = create_sample_token(15, 40_000.0, 50_000.0, 5_000.0, 18_000.0);
    let result_high = engine.evaluate(&high_mcap);
    assert!(!result_high.passed);
}

#[test]
fn test_token_volume_ratio_insufficient() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    // Total volume $15k on $20k MCap (0.75x, below 1.0x)
    let token = create_sample_token(15, 20_000.0, 15_000.0, 3_000.0, 9_000.0);
    let result = engine.evaluate(&token);
    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("Total volume"));
}

#[test]
fn test_rolling_window_calculation() {
    let mut window = TradeRollingWindow::new();
    window.record_trade(1.0, 150.0);
    window.record_trade(2.0, 300.0);

    assert_eq!(window.total_volume_usd(), 450.0);
    assert_eq!(window.volume_last_5m_usd(), 450.0);
}

#[test]
fn test_alert_deduplication_cooldown() {
    let dedup = AlertDeduplicator::new(60);
    let mint = "TestMint11111111111111111111111111111111111";

    assert!(dedup.should_alert(mint));
    dedup.record_alert(mint);
    // Second check within 60 mins must return false
    assert!(!dedup.should_alert(mint));
}

#[test]
fn test_telegram_html_formatting() {
    let token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    let card = AlertCard::from(&token);
    let html = format_alert_html(&card);

    assert!(html.contains("Pepe Early"));
    assert!(html.contains("Bubblemaps"));
    assert!(html.contains("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"));
    assert!(html.contains("https://axiom.trade/token/"));
    assert!(html.contains("https://dexscreener.com/solana/"));
    assert!(html.contains("https://solscan.io/token/"));
    assert!(html.contains("The official test token with lore"));
}

#[test]
fn test_dev_holding_exceeds_threshold_is_rejected() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    let mut token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    token.dev_holding_pct = 18.5; // Exceeds 15% max

    let result = engine.evaluate(&token);
    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("Dev holding"));
}

#[test]
fn test_top_holder_exceeds_threshold_is_rejected() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    let mut token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    token.top_holder_pct = 19.2; // Exceeds 15% max

    let result = engine.evaluate(&token);
    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("Top holder"));
}

#[test]
fn test_sniper_bundler_exceeds_threshold_is_rejected() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    let mut token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    token.sniper_bundler_pct = 22.0; // Exceeds 15% max

    let result = engine.evaluate(&token);
    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("Sniper/bundler"));
}

#[test]
fn test_holders_below_threshold_is_rejected() {
    let thresholds = FilterThresholds::default();
    let engine = FilterEngine::new(thresholds, axiom_bot::config::thresholds::RevivalThresholds::default());

    let mut token = create_sample_token(15, 20_000.0, 25_000.0, 3_000.0, 9_000.0);
    token.holder_count = 34; // Below 50 min

    let result = engine.evaluate(&token);
    assert!(!result.passed);
    assert!(result.rejection_reason.unwrap().contains("Holders"));
}

#[tokio::test]
async fn test_live_helius_das_metrics_fetch() {
    let _ = dotenvy::dotenv();
    if let Ok(rpc_url) = std::env::var("HELIUS_RPC_URL") {
        let client = axiom_bot::enrichment::HeliusDasClient::new(rpc_url);
        // Bonk mint address
        let mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        let res = client.fetch_token_metrics(mint, None).await;
        assert!(res.is_ok(), "Live Helius DAS call failed: {:?}", res.err());
        let metrics = res.unwrap();
        assert!(metrics.total_supply > 0.0);
        assert!(metrics.holder_count > 0);
    }
}

#[test]
fn test_safety_guard_kill_switch() {
    let settings = axiom_bot::config::safety::SafetySettings::default();
    let guard = axiom_bot::safety::SafetyGuard::new(settings);

    assert!(!guard.is_killed());
    assert!(guard.validate_trade("TestMint", 0.05).is_ok());

    guard.engage_kill_switch();
    assert!(guard.is_killed());
    let err = guard.validate_trade("TestMint", 0.05);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("kill switch"));

    guard.resume();
    assert!(!guard.is_killed());
    assert!(guard.validate_trade("TestMint", 0.05).is_ok());
}

#[test]
fn test_safety_guard_max_sol_per_trade() {
    let settings = axiom_bot::config::safety::SafetySettings {
        max_sol_per_trade: 0.1,
        max_daily_spend_sol: 1.0,
        ..Default::default()
    };
    let guard = axiom_bot::safety::SafetyGuard::new(settings);

    // 0.05 SOL is allowed
    assert!(guard.validate_trade("TestMint", 0.05).is_ok());

    // 0.2 SOL exceeds 0.1 SOL cap
    let res = guard.validate_trade("TestMint", 0.2);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("exceeds hard cap"));
}

#[test]
fn test_safety_guard_daily_spend_limit() {
    let settings = axiom_bot::config::safety::SafetySettings {
        max_sol_per_trade: 0.5,
        max_daily_spend_sol: 0.8,
        ..Default::default()
    };
    let guard = axiom_bot::safety::SafetyGuard::new(settings);

    guard.record_spend("tx1", "MintA", 0.5);
    assert_eq!(guard.get_rolling_24h_spend_sol(), 0.5);

    // Trying to spend 0.4 SOL more (0.5 + 0.4 = 0.9 > 0.8 cap) should fail
    let res = guard.validate_trade("MintB", 0.4);
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Daily spend limit"));
}

#[tokio::test]
async fn test_pre_buy_rug_check_live() {
    let _ = dotenvy::dotenv();
    if let Ok(rpc_url) = std::env::var("HELIUS_RPC_URL") {
        let checker = axiom_bot::execution::PreBuyRugChecker::new(rpc_url);
        // Bonk mint (freeze authority revoked)
        let bonk_mint = "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263";
        let res = checker.verify_token_safety(bonk_mint).await;
        assert!(res.is_ok(), "Pre-buy check on Bonk should pass: {:?}", res.err());
    }
}


#[test]
fn test_revival_evaluation() {
    let thresholds = FilterThresholds::default();
    let revival_thresholds = axiom_bot::config::thresholds::RevivalThresholds::default();
    let engine = FilterEngine::new(thresholds, revival_thresholds);

    // 120m old token (2 hours), fell to $3,500 death MCap, now surged to $8,000 (+128%), 5m vol $2,500, Dev = 0.0%
    let mut token = create_sample_token(120, 8_000.0, 30_000.0, 2_500.0, 3_600.0);
    token.lowest_market_cap_usd = 3_500.0;
    token.dev_holding_pct = 0.0; // Clean CTO

    let result = engine.evaluate_revival(&token);
    assert!(result.passed, "Revival token should pass: {:?}", result.rejection_reason);
}
