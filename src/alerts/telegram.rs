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

use super::formatter::{format_alert_html, format_calls_summary, format_refreshed_alert_html};
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
        Self {
            bot,
            allowed_user_id,
            execution_engine,
            dex_client,
            calls,
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
