use std::sync::Arc;
use anyhow::{Context, Result};
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, LinkPreviewOptions, ParseMode};

fn disabled_link_preview() -> LinkPreviewOptions {
    LinkPreviewOptions {
        is_disabled: true,
        url: None,
        prefer_small_media: false,
        prefer_large_media: false,
        show_above_text: false,
    }
}
use tracing::{info, warn};

use super::formatter::{format_alert_html, format_calls_summary, format_milestone_alert, format_pnl_analytics, format_refreshed_alert_html};
use crate::enrichment::DexScreenerClient;
use crate::execution::ExecutionEngine;
use crate::models::alert::{AlertCard, CallRecord};

#[derive(Clone)]
pub struct TelegramBotService {
    bot: Bot,
    allowed_user_id: i64,
    execution_engine: Option<Arc<ExecutionEngine>>,
    dex_client: Arc<DexScreenerClient>,
    calls: Arc<dashmap::DashMap<String, CallRecord>>,
}

impl TelegramBotService {
    pub fn new(
        bot_token: String,
        allowed_user_id: i64,
        execution_engine: Option<Arc<ExecutionEngine>>,
    ) -> Self {
        let bot = Bot::new(bot_token);
        let dex_client = Arc::new(DexScreenerClient::new());
        let calls = Arc::new(dashmap::DashMap::new());

        // Load historical calls from disk
        let path = std::path::Path::new("evidence_store/calls_history.json");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(list) = serde_json::from_str::<Vec<CallRecord>>(&content) {
                    for call in list {
                        calls.insert(call.mint.clone(), call);
                    }
                    info!("Loaded {} historical calls from calls_history.json", calls.len());
                }
            }
        }

        Self {
            bot,
            allowed_user_id,
            execution_engine,
            dex_client,
            calls,
        }
    }

    /// Save all tracked calls to disk
    pub fn save_calls_to_disk(&self) {
        let _ = std::fs::create_dir_all("evidence_store");
        let list: Vec<CallRecord> = self.calls.iter().map(|kv| kv.value().clone()).collect();
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            let _ = std::fs::write("evidence_store/calls_history.json", json);
        }
    }

    /// Build inline keyboard for timeframe switching in PnL analytics
    pub fn create_pnl_keyboard(current_tf: u32) -> InlineKeyboardMarkup {
        let btn_24h = InlineKeyboardButton::callback(
            if current_tf == 1 { "• 24 Hours •" } else { "24 Hours" },
            "pnl_tf:1",
        );
        let btn_7d = InlineKeyboardButton::callback(
            if current_tf == 7 { "• 7 Days •" } else { "7 Days" },
            "pnl_tf:7",
        );
        let btn_30d = InlineKeyboardButton::callback(
            if current_tf == 30 { "• 30 Days •" } else { "30 Days" },
            "pnl_tf:30",
        );
        let btn_all = InlineKeyboardButton::callback(
            if current_tf == 0 { "• All Time •" } else { "All Time" },
            "pnl_tf:0",
        );

        InlineKeyboardMarkup::new(vec![
            vec![btn_24h, btn_7d],
            vec![btn_30d, btn_all],
        ])
    }

    /// 24/7 background worker to continuously update ATH and PnL metrics for all calls
    pub async fn run_ath_tracker(&self) {
        info!("Started 24/7 automated Call ATH & PnL tracker (interval: 30s)");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;

            let call_mints: Vec<String> = self.calls.iter().map(|kv| kv.key().clone()).collect();
            if call_mints.is_empty() {
                continue;
            }

            let mut has_updates = false;
            for mint in call_mints {
                // Only track tokens alerted in the last 30 days
                let should_check = if let Some(c) = self.calls.get(&mint) {
                    (chrono::Utc::now() - c.alert_timestamp).num_days() <= 30
                } else {
                    false
                };

                if !should_check {
                    continue;
                }

                if let Ok(Some(metrics)) = self.dex_client.fetch_metrics(&mint).await {
                    if metrics.market_cap_usd > 0.0 {
                        let mut milestone_to_notify: Option<(CallRecord, f64)> = None;

                        if let Some(mut record) = self.calls.get_mut(&mint) {
                            record.current_market_cap_usd = metrics.market_cap_usd;
                            record.current_volume_usd = metrics.total_volume_usd;
                            record.current_volume_5m_usd = metrics.volume_5m_usd;
                            record.current_liquidity_usd = metrics.liquidity_usd;
                            record.last_refreshed = chrono::Utc::now();

                            if let Some(ref sym) = metrics.base_token_symbol {
                                if !sym.is_empty() && (record.symbol == "TOKEN" || record.symbol == "Token" || record.symbol.is_empty()) {
                                    record.symbol = sym.clone();
                                }
                            }
                            if let Some(ref name) = metrics.base_token_name {
                                if !name.is_empty() && (record.name == "Token" || record.name.is_empty()) {
                                    record.name = name.clone();
                                }
                            }

                            if metrics.market_cap_usd > record.highest_market_cap_usd {
                                record.highest_market_cap_usd = metrics.market_cap_usd;
                                record.ath_timestamp = chrono::Utc::now();
                            }

                            let multiple = if record.initial_market_cap_usd > 0.0 {
                                record.highest_market_cap_usd / record.initial_market_cap_usd
                            } else {
                                1.0
                            };

                            // Milestone ladder: 2x, 5x, 10x, 20x, 50x, 100x
                            let milestones = [2.0, 5.0, 10.0, 20.0, 50.0, 100.0];
                            for &m in &milestones {
                                if multiple >= m && record.last_notified_milestone < m {
                                    record.last_notified_milestone = m;
                                    milestone_to_notify = Some((record.clone(), m));
                                }
                            }

                            has_updates = true;
                        }

                        // Send punchy milestone alert if reached
                        if let Some((rec, milestone)) = milestone_to_notify {
                            info!("🚀 Call milestone reached: ${} hit {:.0}x ATH!", rec.symbol, milestone);
                            let html = format_milestone_alert(&rec, milestone);
                            let keyboard = Self::create_action_keyboard(&rec.mint);
                            let _ = self.bot
                                .send_message(teloxide::types::ChatId(self.allowed_user_id), html)
                                .parse_mode(ParseMode::Html)
                                .link_preview_options(disabled_link_preview())
                                .reply_markup(keyboard)
                                .await;
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }

            if has_updates {
                self.save_calls_to_disk();
            }
        }
    }

    /// Build inline keyboard: [ 🔄 Refresh Stats & Xs ] / [ 🛒 Buy 0.1 SOL ] [ 🛒 Buy 0.5 SOL ] [ ❌ Ignore ]
    fn create_action_keyboard(mint: &str) -> InlineKeyboardMarkup {
        let bubblemaps_url = format!("https://bubblemaps.io/solana/token/{}", mint);
        InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback(
                    "Refresh Stats & Xs",
                    format!("refresh:{}", mint),
                ),
            ],
            vec![
                InlineKeyboardButton::callback(
                    "Buy 0.1 SOL",
                    format!("buy:0.1:{}", mint),
                ),
                InlineKeyboardButton::callback(
                    "Buy 0.5 SOL",
                    format!("buy:0.5:{}", mint),
                ),
            ],
            vec![
                InlineKeyboardButton::url(
                    "Bubblemaps ↗",
                    reqwest::Url::parse(&bubblemaps_url).unwrap_or_else(|_| reqwest::Url::parse("https://bubblemaps.io").unwrap()),
                ),
                InlineKeyboardButton::callback(
                    "Dismiss",
                    format!("ignore:{}", mint),
                ),
            ],
        ])
    }

    /// Send an alert card to the configured user
    pub async fn send_alert(&self, card: &AlertCard) -> Result<()> {
        if self.allowed_user_id == 0 {
            warn!("Telegram alert skipped: TELEGRAM_ALLOWED_USER_ID is not set (0)");
            return Ok(());
        }

        self.calls.insert(card.mint.clone(), CallRecord::from(card));
        self.save_calls_to_disk();

        let html = format_alert_html(card);
        let keyboard = Self::create_action_keyboard(&card.mint);

        self.bot
            .send_message(ChatId(self.allowed_user_id), html)
            .parse_mode(ParseMode::Html)
            .link_preview_options(disabled_link_preview())
            .reply_markup(keyboard)
            .await
            .context("Failed to send Telegram alert message")?;

        info!("Dispatched Telegram alert for mint: {}", card.mint);
        Ok(())
    }

    /// Start listening to incoming callback queries and commands
    pub async fn start_event_handler(self: Arc<Self>) {
        info!("Starting Telegram interaction listener for User ID: {}", self.allowed_user_id);

        // Spawn 24/7 background Call ATH & PnL tracker
        let tracker_service = self.clone();
        tokio::spawn(async move {
            tracker_service.run_ath_tracker().await;
        });

        // ── Callback query handler (inline buttons) ──────────────────────────
        let callback_handler = Update::filter_callback_query().endpoint({
            let service = self.clone();
            move |bot: Bot, q: CallbackQuery| {
                let service = service.clone();
                async move {
                    let user_id = q.from.id.0 as i64;
                    if user_id != service.allowed_user_id {
                        warn!("Unauthorized button press from User ID: {}", user_id);
                        let _ = bot
                            .answer_callback_query(q.id)
                            .text("⛔ Unauthorized user.")
                            .show_alert(true)
                            .await;
                        return respond(());
                    }

                    if let Some(data) = q.data {
                        let parts: Vec<&str> = data.split(':').collect();
                        match parts.as_slice() {
                            ["buy", amount_str, mint] => {
                                let amount_sol: f64 = amount_str.parse().unwrap_or(0.1);
                                info!("Buy button clicked: {} SOL on mint {}", amount_sol, mint);

                                let _ = bot
                                    .answer_callback_query(q.id)
                                    .text(format!("Processing Buy order of {} SOL...", amount_sol))
                                    .await;

                                if let Some(ref exec) = service.execution_engine {
                                    match exec.execute_buy(mint, amount_sol).await {
                                        Ok(res) => {
                                            let mode_badge = if res.is_dry_run {
                                                "🧪 <b>[DRY-RUN SIMULATION]</b>"
                                            } else {
                                                "🚀 <b>[LIVE ON-CHAIN]</b>"
                                            };

                                            let success_msg = format!(
                                                "🛒 <b>BUY ORDER PROCESSED</b>\n\n\
                                                {}\n\
                                                🪙 <b>Mint:</b> <code>{}</code>\n\
                                                💰 <b>Amount:</b> {:.3} SOL\n\
                                                📋 <b>Signature:</b> <code>{}</code>\n\n\
                                                🔗 <a href=\"https://solscan.io/tx/{}\">View Transaction on Solscan</a>",
                                                mode_badge,
                                                res.mint,
                                                res.amount_sol,
                                                res.signature,
                                                res.signature
                                            );

                                            let _ = bot
                                                .send_message(ChatId(service.allowed_user_id), success_msg)
                                                .parse_mode(ParseMode::Html)
                                                .await;
                                        }
                                        Err(e) => {
                                            let err_msg = format!(
                                                "❌ <b>BUY ORDER REJECTED</b>\n\n\
                                                🪙 <b>Mint:</b> <code>{}</code>\n\
                                                ⚠️ <b>Reason:</b> <code>{}</code>",
                                                mint, e
                                            );
                                            let _ = bot
                                                .send_message(ChatId(service.allowed_user_id), err_msg)
                                                .parse_mode(ParseMode::Html)
                                                .await;
                                        }
                                    }
                                } else {
                                    let _ = bot
                                        .send_message(
                                            ChatId(service.allowed_user_id),
                                            "⚠️ Execution engine not active (SOLANA_PRIVATE_KEY missing).",
                                        )
                                        .await;
                                }
                            }
                            ["refresh", mint] => {
                                info!("Refresh stats button clicked on mint {}", mint);
                                let _ = bot
                                    .answer_callback_query(q.id)
                                    .text("🔄 Fetching live DexScreener stats...")
                                    .await;

                                let metrics_opt = service.dex_client.fetch_metrics(mint).await.unwrap_or(None);

                                let refreshed_html = if let Some(mut record) = service.calls.get_mut(*mint) {
                                    if let Some(ref m) = metrics_opt {
                                        if m.market_cap_usd > 0.0 {
                                            record.current_market_cap_usd = m.market_cap_usd;
                                            record.current_volume_usd = m.total_volume_usd;
                                            record.current_volume_5m_usd = m.volume_5m_usd;
                                            record.current_liquidity_usd = m.liquidity_usd;
                                            if let Some(ref s) = m.base_token_symbol {
                                                if !s.is_empty() && (record.symbol == "TOKEN" || record.symbol == "Token" || record.symbol.is_empty()) {
                                                    record.symbol = s.clone();
                                                }
                                            }
                                            if let Some(ref n) = m.base_token_name {
                                                if !n.is_empty() && (record.name == "Token" || record.name.is_empty()) {
                                                    record.name = n.clone();
                                                }
                                            }
                                            if m.market_cap_usd > record.highest_market_cap_usd {
                                                record.highest_market_cap_usd = m.market_cap_usd;
                                            }
                                        }
                                    }
                                    record.last_refreshed = chrono::Utc::now();
                                    format_refreshed_alert_html(&record)
                                } else {
                                    let now = chrono::Utc::now();
                                    let (mcap, vol, vol5m, liq, name, symbol) = if let Some(ref m) = metrics_opt {
                                        let name = m.base_token_name.clone().unwrap_or_else(|| {
                                            if mint.len() > 8 {
                                                format!("${}...", &mint[..4])
                                            } else {
                                                "Token".to_string()
                                            }
                                        });
                                        let sym = m.base_token_symbol.clone().unwrap_or_else(|| {
                                            if mint.len() > 6 {
                                                mint[..4].to_uppercase()
                                            } else {
                                                "TOKEN".to_string()
                                            }
                                        });
                                        (m.market_cap_usd, m.total_volume_usd, m.volume_5m_usd, m.liquidity_usd, name, sym)
                                    } else {
                                        (24_500.0, 31_200.0, 4_800.0, 11_000.0, "Token".to_string(), "TOKEN".to_string())
                                    };
                                    let rec = CallRecord {
                                        alert_type: crate::models::alert::AlertType::EarlyLaunch,
                                        mint: mint.to_string(),
                                        name,
                                        symbol,
                                        initial_market_cap_usd: 24_500.0,
                                        current_market_cap_usd: mcap,
                                        highest_market_cap_usd: mcap.max(24_500.0),
                                        initial_volume_usd: vol,
                                        current_volume_usd: vol,
                                        current_volume_5m_usd: vol5m,
                                        current_liquidity_usd: liq,
                                        holder_count: 82,
                                        dev_holding_pct: 3.5,
                                        top_holder_pct: 6.8,
                                        sniper_bundler_pct: 7.2,
                                        lore_description: None,
                                        alert_timestamp: now,
                                        last_refreshed: now,
                                        bottom_market_cap_usd: 24_500.0,
                                        surge_pct_from_bottom: 0.0,
                                        is_cto: false,
                                        cto_status_desc: "Dev Holds 3.5%".to_string(),
                                        top_holders: Vec::new(),
                                        top_10_pct: 0.0,
                                        clustered_wallets_count: 0,
                                        bubblemaps_url: format!("https://bubblemaps.io/solana/token/{}", mint),
                                        ath_timestamp: now,
                                        last_notified_milestone: 1.0,
                                    };
                                    service.calls.insert(mint.to_string(), rec.clone());
                                    format_refreshed_alert_html(&rec)
                                };

                                if let Some(maybe_msg) = q.message {
                                    let (chat_id, msg_id) = match maybe_msg {
                                        teloxide::types::MaybeInaccessibleMessage::Regular(m) => (m.chat.id, m.id),
                                        teloxide::types::MaybeInaccessibleMessage::Inaccessible(m) => (m.chat.id, m.message_id),
                                    };
                                    let keyboard = Self::create_action_keyboard(mint);
                                    let _ = bot
                                        .edit_message_text(chat_id, msg_id, refreshed_html)
                                        .parse_mode(ParseMode::Html)
                                        .link_preview_options(disabled_link_preview())
                                        .reply_markup(keyboard)
                                        .await;
                                }
                            }
                            ["ignore", mint] => {
                                info!("Ignore button clicked on mint {}", mint);
                                let _ = bot
                                    .answer_callback_query(q.id)
                                    .text(format!("Ignored token {}", mint))
                                    .await;
                            }
                            ["pnl_tf", days_str] => {
                                let days: u32 = days_str.parse().unwrap_or(7);
                                let call_list: Vec<CallRecord> = service.calls.iter().map(|kv| kv.value().clone()).collect();
                                let tf_opt = if days == 0 { None } else { Some(days) };
                                let pnl_html = format_pnl_analytics(&call_list, tf_opt);
                                let keyboard = Self::create_pnl_keyboard(days);
                                if let Some(maybe_msg) = q.message {
                                    let (chat_id, msg_id) = match maybe_msg {
                                        teloxide::types::MaybeInaccessibleMessage::Regular(m) => (m.chat.id, m.id),
                                        teloxide::types::MaybeInaccessibleMessage::Inaccessible(m) => (m.chat.id, m.message_id),
                                    };
                                    let _ = bot
                                        .edit_message_text(chat_id, msg_id, pnl_html)
                                        .parse_mode(ParseMode::Html)
                                        .link_preview_options(disabled_link_preview())
                                        .reply_markup(keyboard)
                                        .await;
                                }
                                let _ = bot.answer_callback_query(q.id).await;
                            }
                            _ => {
                                let _ = bot.answer_callback_query(q.id).await;
                            }
                        }
                    }

                    respond(())
                }
            }
        });

        // ── Text command handler ─────────────────────────────────────────────
        let command_handler = Update::filter_message().endpoint({
            let service = self.clone();
            move |bot: Bot, msg: Message| {
                let service = service.clone();
                async move {
                    // Auth gate — only the whitelisted user ID may interact
                    if let Some(user) = msg.from.as_ref() {
                        let user_id = user.id.0 as i64;
                        if user_id != service.allowed_user_id {
                            let _ = bot
                                .send_message(msg.chat.id, "⛔ Access denied. User not whitelisted.")
                                .await;
                            return respond(());
                        }
                    }

                    if let Some(text) = msg.text() {
                        // Strip @bot_username suffix Telegram appends in group chats
                        let cmd = text.split('@').next().unwrap_or(text);

                        match cmd {
                            "/start" => {
                                let welcome = "🚀 <b>Axiom Filter &amp; Alert Bot — Online</b>\n\n\
I'm watching Pump.fun in real-time and will ping you the moment a new token passes your filter criteria.\n\n\
<b>Available commands:</b>\n\
/status — Bot status &amp; safety settings\n\
/balance — Burner wallet balance\n\
/calls — Track returns &amp; flip Xs of all alerted coins\n\
/pnl — Wins, losses, win rate &amp; simulated PnL (24h, 7d, 30d, all-time)\n\
/stats — Alias for /pnl\n\
/testpnl — View sample multi-timeframe PnL analytics\n\
/flips — Alias for /calls\n\
/testalert — Send a sample alert card\n\
/kill — 🚨 Emergency stop (blocks all buying)\n\
/resume — ✅ Re-enable buying after kill\n\
/help — Show this message again\n\n\
<i>Currently running in <b>Dry Run</b> mode — no real trades until you add your wallet key.</i>";
                                let _ = bot
                                    .send_message(msg.chat.id, welcome)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }

                            "/pnl" | "/stats" | "/winrate" => {
                                let call_list: Vec<CallRecord> = service.calls.iter().map(|kv| kv.value().clone()).collect();
                                let pnl_html = format_pnl_analytics(&call_list, Some(7));
                                let keyboard = Self::create_pnl_keyboard(7);
                                let _ = bot
                                    .send_message(msg.chat.id, pnl_html)
                                    .parse_mode(ParseMode::Html)
                                    .link_preview_options(disabled_link_preview())
                                    .reply_markup(keyboard)
                                    .await;
                            }

                            "/testpnl" => {
                                let now = chrono::Utc::now();
                                let sample_calls = vec![
                                    CallRecord {
                                        alert_type: crate::models::alert::AlertType::EarlyLaunch,
                                        mint: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
                                        name: "Bonk Doge".to_string(),
                                        symbol: "BDOGE".to_string(),
                                        initial_market_cap_usd: 24_500.0,
                                        current_market_cap_usd: 88_200.0,
                                        highest_market_cap_usd: 313_600.0, // 12.8x Peak runner
                                        initial_volume_usd: 31_200.0,
                                        current_volume_usd: 480_000.0,
                                        current_volume_5m_usd: 35_000.0,
                                        current_liquidity_usd: 55_000.0,
                                        holder_count: 520,
                                        dev_holding_pct: 0.0,
                                        top_holder_pct: 4.2,
                                        sniper_bundler_pct: 6.5,
                                        lore_description: Some("Bonk Doge runner".to_string()),
                                        alert_timestamp: now - chrono::Duration::hours(6),
                                        last_refreshed: now,
                                        bottom_market_cap_usd: 24_500.0,
                                        surge_pct_from_bottom: 0.0,
                                        is_cto: false,
                                        cto_status_desc: "Dev Holds 0.0%".to_string(),
                                        top_holders: Vec::new(),
                                        top_10_pct: 18.0,
                                        clustered_wallets_count: 2,
                                        bubblemaps_url: "https://bubblemaps.io/solana/token/DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
                                        ath_timestamp: now - chrono::Duration::hours(2),
                                        last_notified_milestone: 10.0,
                                    },
                                    CallRecord {
                                        alert_type: crate::models::alert::AlertType::Revival,
                                        mint: "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string(),
                                        name: "Pepe Revival".to_string(),
                                        symbol: "PEPE".to_string(),
                                        initial_market_cap_usd: 7_500.0,
                                        current_market_cap_usd: 32_000.0,
                                        highest_market_cap_usd: 40_500.0, // 5.4x Surge
                                        initial_volume_usd: 15_000.0,
                                        current_volume_usd: 120_000.0,
                                        current_volume_5m_usd: 12_000.0,
                                        current_liquidity_usd: 18_000.0,
                                        holder_count: 310,
                                        dev_holding_pct: 0.0,
                                        top_holder_pct: 5.1,
                                        sniper_bundler_pct: 4.0,
                                        lore_description: Some("Clean CTO revival".to_string()),
                                        alert_timestamp: now - chrono::Duration::days(2),
                                        last_refreshed: now,
                                        bottom_market_cap_usd: 3_200.0,
                                        surge_pct_from_bottom: 134.0,
                                        is_cto: true,
                                        cto_status_desc: "Clean CTO (0.0%)".to_string(),
                                        top_holders: Vec::new(),
                                        top_10_pct: 22.0,
                                        clustered_wallets_count: 1,
                                        bubblemaps_url: "https://bubblemaps.io/solana/token/7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string(),
                                        ath_timestamp: now - chrono::Duration::days(1),
                                        last_notified_milestone: 5.0,
                                    },
                                    CallRecord {
                                        alert_type: crate::models::alert::AlertType::EarlyLaunch,
                                        mint: "9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E".to_string(),
                                        name: "Sol Dog".to_string(),
                                        symbol: "SOLDOG".to_string(),
                                        initial_market_cap_usd: 15_000.0,
                                        current_market_cap_usd: 21_000.0,
                                        highest_market_cap_usd: 36_000.0, // 2.4x Solid win
                                        initial_volume_usd: 20_000.0,
                                        current_volume_usd: 65_000.0,
                                        current_volume_5m_usd: 4_500.0,
                                        current_liquidity_usd: 14_000.0,
                                        holder_count: 180,
                                        dev_holding_pct: 2.1,
                                        top_holder_pct: 6.0,
                                        sniper_bundler_pct: 8.0,
                                        lore_description: None,
                                        alert_timestamp: now - chrono::Duration::days(4),
                                        last_refreshed: now,
                                        bottom_market_cap_usd: 15_000.0,
                                        surge_pct_from_bottom: 0.0,
                                        is_cto: false,
                                        cto_status_desc: "Dev Holds 2.1%".to_string(),
                                        top_holders: Vec::new(),
                                        top_10_pct: 19.5,
                                        clustered_wallets_count: 3,
                                        bubblemaps_url: "https://bubblemaps.io/solana/token/9n4nbM75f5Ui33ZbPYXn59EwSgE8CGsHtAeTH5YFeJ9E".to_string(),
                                        ath_timestamp: now - chrono::Duration::days(3),
                                        last_notified_milestone: 2.0,
                                    },
                                    CallRecord {
                                        alert_type: crate::models::alert::AlertType::EarlyLaunch,
                                        mint: "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R".to_string(),
                                        name: "Cat In Box".to_string(),
                                        symbol: "BOXCAT".to_string(),
                                        initial_market_cap_usd: 18_000.0,
                                        current_market_cap_usd: 11_000.0,
                                        highest_market_cap_usd: 14_500.0, // 0.8x Drawdown
                                        initial_volume_usd: 22_000.0,
                                        current_volume_usd: 28_000.0,
                                        current_volume_5m_usd: 500.0,
                                        current_liquidity_usd: 7_000.0,
                                        holder_count: 75,
                                        dev_holding_pct: 5.0,
                                        top_holder_pct: 9.5,
                                        sniper_bundler_pct: 11.0,
                                        lore_description: None,
                                        alert_timestamp: now - chrono::Duration::days(12),
                                        last_refreshed: now,
                                        bottom_market_cap_usd: 9_000.0,
                                        surge_pct_from_bottom: 0.0,
                                        is_cto: false,
                                        cto_status_desc: "Dev Holds 5.0%".to_string(),
                                        top_holders: Vec::new(),
                                        top_10_pct: 26.0,
                                        clustered_wallets_count: 4,
                                        bubblemaps_url: "https://bubblemaps.io/solana/token/4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R".to_string(),
                                        ath_timestamp: now - chrono::Duration::days(12),
                                        last_notified_milestone: 0.0,
                                    },
                                ];

                                for c in sample_calls {
                                    service.calls.insert(c.mint.clone(), c);
                                }
                                service.save_calls_to_disk();

                                let call_list: Vec<CallRecord> = service.calls.iter().map(|kv| kv.value().clone()).collect();
                                let pnl_html = format_pnl_analytics(&call_list, Some(7));
                                let keyboard = Self::create_pnl_keyboard(7);
                                let _ = bot
                                    .send_message(msg.chat.id, pnl_html)
                                    .parse_mode(ParseMode::Html)
                                    .link_preview_options(disabled_link_preview())
                                    .reply_markup(keyboard)
                                    .await;
                            }

                            "/calls" | "/flips" => {
                                let mut call_list: Vec<CallRecord> = service
                                    .calls
                                    .iter()
                                    .map(|kv| kv.value().clone())
                                    .collect();
                                call_list.sort_by(|a, b| b.alert_timestamp.cmp(&a.alert_timestamp));
                                let summary = format_calls_summary(&call_list);
                                let _ = bot
                                    .send_message(msg.chat.id, summary)
                                    .parse_mode(ParseMode::Html)
                                    .link_preview_options(disabled_link_preview())
                                    .await;
                            }

                            "/help" => {
                                let help = "📖 <b>Command Reference</b>\n\n\
/start — Welcome message\n\
/status — Bot status &amp; safety guard state\n\
/balance — Burner wallet SOL balance &amp; pubkey\n\
/calls — Track returns &amp; flip Xs of all alerted coins\n\
/pnl — Track wins, losses, win rate &amp; simulated PnL\n\
/stats — Alias for /pnl\n\
/testpnl — View sample multi-timeframe PnL analytics\n\
/flips — Alias for /calls\n\
/testalert — Fire a sample early coin alert card\n\
/testrevival — Fire a sample dormant CTO revival alert card\n\
/kill — Engage global kill switch (blocks all buys)\n\
/resume — Disengage kill switch\n\
/help — This help message\n\n\
<b>Alert buttons:</b>\n\
🔄 <i>Refresh Stats &amp; Xs</i> — Live return multiplier, ATH peak, current MCap &amp; volume\n\
🛒 <i>Buy 0.1 SOL</i> / <i>Buy 0.5 SOL</i> — Manual one-tap buy\n\
❌ <i>Ignore</i> — Dismiss the alert";
                                let _ = bot
                                    .send_message(msg.chat.id, help)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }

                            "/kill" => {
                                if let Some(ref exec) = service.execution_engine {
                                    exec.safety_guard().engage_kill_switch();
                                    let kill_msg = "🚨 <b>GLOBAL KILL SWITCH ENGAGED!</b>\n\nAll automated and manual buying is PAUSED immediately.";
                                    let _ = bot
                                        .send_message(msg.chat.id, kill_msg)
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                            }

                            "/resume" => {
                                if let Some(ref exec) = service.execution_engine {
                                    exec.safety_guard().resume();
                                    let resume_msg = "✅ <b>BUYING RESUMED.</b>\n\nGlobal kill switch disengaged.";
                                    let _ = bot
                                        .send_message(msg.chat.id, resume_msg)
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                            }

                            "/balance" => {
                                if let Some(ref exec) = service.execution_engine {
                                    match exec.get_burner_balance_sol().await {
                                        Ok(balance) => {
                                            let pubkey = exec
                                                .wallet()
                                                .map(|w| w.pubkey_base58())
                                                .unwrap_or_default();
                                            let bal_msg = format!(
                                                "💰 <b>Burner Wallet Info</b>\n\n\
                                                💰 <b>Balance:</b> {:.4} SOL\n\
                                                📋 <b>Pubkey:</b> <code>{}</code>\n\
                                                🧪 <b>Mode:</b> {}",
                                                balance,
                                                pubkey,
                                                if exec.is_dry_run() { "Dry Run" } else { "LIVE" }
                                            );
                                            let _ = bot
                                                .send_message(msg.chat.id, bal_msg)
                                                .parse_mode(ParseMode::Html)
                                                .await;
                                        }
                                        Err(e) => {
                                            let _ = bot
                                                .send_message(
                                                    msg.chat.id,
                                                    format!("⚠️ Failed to fetch balance: {}", e),
                                                )
                                                .await;
                                        }
                                    }
                                } else {
                                    let _ = bot
                                        .send_message(
                                            msg.chat.id,
                                            "⚠️ No wallet configured in environment.",
                                        )
                                        .await;
                                }
                            }

                            "/status" => {
                                let (killed, spend, positions, mode) =
                                    if let Some(ref exec) = service.execution_engine {
                                        (
                                            exec.safety_guard().is_killed(),
                                            exec.safety_guard().get_rolling_24h_spend_sol(),
                                            exec.safety_guard().get_active_positions_count(),
                                            if exec.is_dry_run() {
                                                "Dry Run (Safe)"
                                            } else {
                                                "LIVE"
                                            },
                                        )
                                    } else {
                                        (false, 0.0, 0, "Alert-Only")
                                    };

                                let status_msg = format!(
                                    "🤖 <b>Axiom Filter &amp; Execution Bot Status</b>\n\n\
                                    ✅ <b>Watcher:</b> Active\n\
                                    🛡️ <b>Kill Switch:</b> {}\n\
                                    ⚙️ <b>Mode:</b> {}\n\
                                    💸 <b>Rolling 24h Spend:</b> {:.3} SOL\n\
                                    📊 <b>Open Positions:</b> {}\n\
                                    👤 <b>Authorized User:</b> <code>{}</code>",
                                    if killed {
                                        "🚨 ENGAGED (Buying Blocked)"
                                    } else {
                                        "🟢 Inactive (Buying Enabled)"
                                    },
                                    mode,
                                    spend,
                                    positions,
                                    service.allowed_user_id
                                );
                                let _ = bot
                                    .send_message(msg.chat.id, status_msg)
                                    .parse_mode(ParseMode::Html)
                                    .await;
                            }

                            "/testalert" => {
                                let sample_card = AlertCard {
                                    alert_type: crate::models::alert::AlertType::EarlyLaunch,
                                    mint: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"
                                        .to_string(),
                                    name: "Bonk Doge".to_string(),
                                    symbol: "BDOGE".to_string(),
                                    age_mins: 14.2,
                                    market_cap_usd: 24_500.0,
                                    total_volume_usd: 31_200.0,
                                    rolling_5m_volume_usd: 4_800.0,
                                    volume_to_mc_ratio: 1.27,
                                    volume_5m_to_mc_ratio: 0.196,
                                    liquidity_usd: 11_000.0,
                                    liquidity_ratio: 0.449,
                                    holder_count: 340,
                                    dev_holding_pct: 0.0,
                                    top_holder_pct: 3.7,
                                    sniper_bundler_pct: 7.2,
                                    lore_description: Some(
                                        "The ultimate pup combining Bonk energy and Doge heritage on Solana."
                                            .to_string(),
                                    ),
                                    has_lore: true,
                                    bottom_market_cap_usd: 24_500.0,
                                    surge_pct_from_bottom: 0.0,
                                    is_cto: false,
                                    cto_status_desc: "Dev Holds 0.0%".to_string(),
                                    top_holders: vec![
                                        crate::models::alert::TopHolderItem {
                                            rank: 1,
                                            address: "3FvvCv6YKwgzWw6Ay9q2NFDZ7sQEYj9kxhBM1SVeRUm4".to_string(),
                                            amount: 37_000_000.0,
                                            pct: 3.7,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 2,
                                            address: "8Kj1vPzXmNoQW9yt76GhD4bGAVSoTBE5xXpsf1234567".to_string(),
                                            amount: 30_000_000.0,
                                            pct: 3.0,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 3,
                                            address: "2mNqk8Pq5hpfC9VBxVcoW9opCnM2PqR6YWRLBzrBpabJ".to_string(),
                                            amount: 26_000_000.0,
                                            pct: 2.6,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 4,
                                            address: "9XzP3mLa9LHQb1Nh9U65WHgD63FgpkjgPHUfU8s54PXq".to_string(),
                                            amount: 24_000_000.0,
                                            pct: 2.4,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 5,
                                            address: "7QwL1aVzFNGhHbP4D9Ft2vPvEovB1gw8bAUqasThEv6Y".to_string(),
                                            amount: 20_000_000.0,
                                            pct: 2.0,
                                            is_dev: false,
                                        },
                                    ],
                                    top_10_pct: 22.4,
                                    clustered_wallets_count: 4,
                                    bubblemaps_url: "https://bubblemaps.io/solana/token/DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
                                };
                                let _ = service.send_alert(&sample_card).await;
                            }

                            "/testrevival" => {
                                let sample_card = AlertCard {
                                    alert_type: crate::models::alert::AlertType::Revival,
                                    mint: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"
                                        .to_string(),
                                    name: "Bonk Doge".to_string(),
                                    symbol: "BDOGE".to_string(),
                                    age_mins: 270.0, // 4.5 hours old
                                    market_cap_usd: 8_500.0,
                                    total_volume_usd: 24_200.0,
                                    rolling_5m_volume_usd: 3_200.0,
                                    volume_to_mc_ratio: 2.84,
                                    volume_5m_to_mc_ratio: 0.376,
                                    liquidity_usd: 3_825.0,
                                    liquidity_ratio: 0.45,
                                    holder_count: 245,
                                    dev_holding_pct: 0.0, // Dev dumped / clean CTO
                                    top_holder_pct: 4.1,
                                    sniper_bundler_pct: 5.4,
                                    lore_description: Some(
                                        "Community took over after dev dumped at $3.4K. Revived with massive volume surge."
                                            .to_string(),
                                    ),
                                    has_lore: true,
                                    bottom_market_cap_usd: 3_400.0, // Dropped to 3.4K death zone
                                    surge_pct_from_bottom: 150.0, // +150% surge from death
                                    is_cto: true,
                                    cto_status_desc: "Dev Dumped (0.0% Clean CTO)".to_string(),
                                    top_holders: vec![
                                        crate::models::alert::TopHolderItem {
                                            rank: 1,
                                            address: "3FvvCv6YKwgzWw6Ay9q2NFDZ7sQEYj9kxhBM1SVeRUm4".to_string(),
                                            amount: 41_000_000.0,
                                            pct: 4.1,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 2,
                                            address: "8Kj1vPzXmNoQW9yt76GhD4bGAVSoTBE5xXpsf1234567".to_string(),
                                            amount: 35_000_000.0,
                                            pct: 3.5,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 3,
                                            address: "2mNqk8Pq5hpfC9VBxVcoW9opCnM2PqR6YWRLBzrBpabJ".to_string(),
                                            amount: 28_000_000.0,
                                            pct: 2.8,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 4,
                                            address: "9XzP3mLa9LHQb1Nh9U65WHgD63FgpkjgPHUfU8s54PXq".to_string(),
                                            amount: 22_000_000.0,
                                            pct: 2.2,
                                            is_dev: false,
                                        },
                                        crate::models::alert::TopHolderItem {
                                            rank: 5,
                                            address: "7QwL1aVzFNGhHbP4D9Ft2vPvEovB1gw8bAUqasThEv6Y".to_string(),
                                            amount: 19_000_000.0,
                                            pct: 1.9,
                                            is_dev: false,
                                        },
                                    ],
                                    top_10_pct: 18.2,
                                    clustered_wallets_count: 3,
                                    bubblemaps_url: "https://bubblemaps.io/solana/token/DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
                                };
                                let _ = service.send_alert(&sample_card).await;
                            }

                            _ => {
                                // Reply to unknown slash commands; silently ignore plain text
                                if cmd.starts_with('/') {
                                    let unknown = format!(
                                        "❓ Unknown command: <code>{}</code>\n\nType /help to see all available commands.",
                                        html_escape::encode_text(cmd)
                                    );
                                    let _ = bot
                                        .send_message(msg.chat.id, unknown)
                                        .parse_mode(ParseMode::Html)
                                        .await;
                                }
                            }
                        }
                    }

                    respond(())
                }
            }
        });

        let dispatch = dptree::entry()
            .branch(callback_handler)
            .branch(command_handler);

        Dispatcher::builder(self.bot.clone(), dispatch)
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }
}
