use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use chrono::Utc;
use dashmap::DashMap;
use tracing::debug;

use crate::models::events::{NewTokenPayload, TradePayload};
use crate::models::token::TokenState;
use super::rolling_window::TradeRollingWindow;

pub struct TokenStore {
    tokens: DashMap<String, TokenState>,
    windows: DashMap<String, TradeRollingWindow>,
    // Track unique traders seen per mint for baseline holder count before Helius DAS
    buyers: DashMap<String, dashmap::DashSet<String>>,
    // SOL price in USD stored as cents
    sol_price_cents: AtomicU64,
}

impl TokenStore {
    pub fn new(initial_sol_price_usd: f64) -> Arc<Self> {
        let cents = (initial_sol_price_usd * 100.0) as u64;
        Arc::new(Self {
            tokens: DashMap::new(),
            windows: DashMap::new(),
            buyers: DashMap::new(),
            sol_price_cents: AtomicU64::new(cents),
        })
    }

    pub fn sol_price_usd(&self) -> f64 {
        self.sol_price_cents.load(Ordering::Relaxed) as f64 / 100.0
    }

    pub fn set_sol_price_usd(&self, price: f64) {
        let cents = (price * 100.0) as u64;
        self.sol_price_cents.store(cents, Ordering::Relaxed);
    }

    pub fn insert_new_token(&self, payload: NewTokenPayload) {
        let sol_price = self.sol_price_usd();
        let mcap_sol = payload.market_cap_sol.unwrap_or(30.0);
        let name = payload.name.unwrap_or_else(|| "Unknown".to_string());
        let symbol = payload.symbol.unwrap_or_else(|| "UNKNOWN".to_string());
        let uri = payload.uri.unwrap_or_default();

        let mut token = TokenState::new(
            payload.mint.clone(),
            name,
            symbol,
            uri,
            payload.trader_public_key.clone(),
            mcap_sol,
            sol_price,
        );

        let initial_sol = payload.sol_amount.unwrap_or(0.0);
        if initial_sol > 0.0 {
            let mut window = TradeRollingWindow::new();
            let usd = initial_sol * sol_price;
            window.record_trade(initial_sol, usd);
            token.total_volume_sol = initial_sol;
            token.total_volume_usd = usd;
            token.rolling_5m_volume_sol = initial_sol;
            token.rolling_5m_volume_usd = usd;
            self.windows.insert(payload.mint.clone(), window);
        } else {
            self.windows.insert(payload.mint.clone(), TradeRollingWindow::new());
        }

        let buyers_set = dashmap::DashSet::new();
        if let Some(dev) = payload.trader_public_key {
            buyers_set.insert(dev);
        }
        token.holder_count = buyers_set.len().max(1) as u64;
        self.buyers.insert(payload.mint.clone(), buyers_set);

        self.tokens.insert(payload.mint, token);
    }

    pub fn record_trade(&self, payload: TradePayload) {
        let sol_price = self.sol_price_usd();
        let sol_amount = payload.sol_amount.unwrap_or(0.0).abs();
        let usd_amount = sol_amount * sol_price;

        if let Some(trader) = &payload.trader_public_key {
            self.buyers
                .entry(payload.mint.clone())
                .or_default()
                .insert(trader.clone());
        }

        let mut window = self.windows.entry(payload.mint.clone()).or_default();
        window.record_trade(sol_amount, usd_amount);
        let vol_5m_usd = window.volume_last_5m_usd();
        let vol_5m_sol = window.volume_last_5m_sol();
        let total_vol_usd = window.total_volume_usd();
        let total_vol_sol = window.total_volume_sol();
        drop(window);

        if let Some(mut token) = self.tokens.get_mut(&payload.mint) {
            token.last_updated = Utc::now();
            if let Some(mcap_sol) = payload.market_cap_sol {
                token.market_cap_sol = mcap_sol;
                token.market_cap_usd = mcap_sol * sol_price;
                token.liquidity_sol = mcap_sol * 0.45;
                token.liquidity_usd = token.liquidity_sol * sol_price;
                if token.lowest_market_cap_usd == 0.0 || token.market_cap_usd < token.lowest_market_cap_usd {
                    token.lowest_market_cap_usd = token.market_cap_usd;
                }
            }

            token.rolling_5m_volume_sol = vol_5m_sol;
            token.rolling_5m_volume_usd = vol_5m_usd;
            token.total_volume_sol = total_vol_sol;
            token.total_volume_usd = total_vol_usd;

            if let Some(buyers) = self.buyers.get(&payload.mint) {
                token.holder_count = buyers.len().max(token.holder_count as usize) as u64;
            }
        }
    }

    pub fn update_lore(&self, mint: &str, description: String, min_length: usize) {
        if let Some(mut token) = self.tokens.get_mut(mint) {
            let is_substantial = description.trim().len() >= min_length;
            token.has_lore = is_substantial;
            token.description = Some(description);
        }
    }

    pub fn get_token(&self, mint: &str) -> Option<TokenState> {
        self.tokens.get(mint).map(|t| t.clone())
    }

    pub fn update_token_enrichment(&self, updated: &TokenState) {
        if let Some(mut token) = self.tokens.get_mut(&updated.mint) {
            token.holder_count = updated.holder_count;
            token.dev_holding_pct = updated.dev_holding_pct;
            token.top_holder_pct = updated.top_holder_pct;
            token.top_10_pct = updated.top_10_pct;
            token.top_holders = updated.top_holders.clone();
            token.sniper_bundler_pct = updated.sniper_bundler_pct;
            token.clustered_wallets_count = updated.clustered_wallets_count;
            if updated.market_cap_usd > 0.0 {
                token.market_cap_usd = updated.market_cap_usd;
                token.market_cap_sol = updated.market_cap_sol;
                token.liquidity_usd = updated.liquidity_usd;
                token.liquidity_sol = updated.liquidity_sol;
                if token.lowest_market_cap_usd == 0.0 || token.market_cap_usd < token.lowest_market_cap_usd {
                    token.lowest_market_cap_usd = token.market_cap_usd;
                }
            }
            if updated.total_volume_usd > 0.0 {
                token.total_volume_usd = updated.total_volume_usd;
                token.rolling_5m_volume_usd = updated.rolling_5m_volume_usd;
            }
        }
    }

    pub fn get_active_tokens(&self) -> Vec<TokenState> {
        // First refresh sliding 5m volume for all active tokens
        for mut token in self.tokens.iter_mut() {
            if let Some(window) = self.windows.get(&token.mint) {
                token.rolling_5m_volume_usd = window.volume_last_5m_usd();
                token.rolling_5m_volume_sol = window.volume_last_5m_sol();
                token.total_volume_usd = window.total_volume_usd();
                token.total_volume_sol = window.total_volume_sol();
            }
        }

        self.tokens.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn mark_alert_sent(&self, mint: &str) {
        if let Some(mut token) = self.tokens.get_mut(mint) {
            token.alert_sent_at = Some(Utc::now());
        }
    }

    /// Prune tokens whose age exceeds max_age_mins to prevent memory leaks
    pub fn prune_expired_tokens(&self, max_age_mins: u64) {
        let max_age_f64 = max_age_mins as f64;
        let mut to_remove = Vec::new();

        for entry in self.tokens.iter() {
            if entry.value().age_mins() > max_age_f64 {
                to_remove.push(entry.key().clone());
            }
        }

        for mint in to_remove {
            debug!("Pruning expired token: {}", mint);
            self.tokens.remove(&mint);
            self.windows.remove(&mint);
            self.buyers.remove(&mint);
        }
    }
}
