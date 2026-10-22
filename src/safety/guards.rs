use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{bail, Result};
use dashmap::DashMap;
use tracing::{info, warn};

use crate::config::safety::SafetySettings;

pub struct SafetyGuard {
    settings: SafetySettings,
    kill_switch_active: AtomicBool,
    // Daily spend tracking: (timestamp, sol_spent)
    recent_spends: Arc<DashMap<String, (Instant, f64)>>,
    active_positions: Arc<DashMap<String, f64>>,
}

impl SafetyGuard {
    pub fn new(settings: SafetySettings) -> Self {
        Self {
            settings,
            kill_switch_active: AtomicBool::new(false),
            recent_spends: Arc::new(DashMap::new()),
            active_positions: Arc::new(DashMap::new()),
        }
    }

    /// Check if kill switch is engaged
    pub fn is_killed(&self) -> bool {
        self.kill_switch_active.load(Ordering::SeqCst)
    }

    /// Activate kill switch (pauses all buying immediately)
    pub fn engage_kill_switch(&self) {
        self.kill_switch_active.store(true, Ordering::SeqCst);
        warn!("🚨 GLOBAL KILL SWITCH ENGAGED! All buying paused.");
    }

    /// Deactivate kill switch
    pub fn resume(&self) {
        self.kill_switch_active.store(false, Ordering::SeqCst);
        info!("✅ Global kill switch disengaged. Buying resumed.");
    }

    /// Calculate rolling 24-hour spend in SOL
    pub fn get_rolling_24h_spend_sol(&self) -> f64 {
        let now = Instant::now();
        let day = Duration::from_secs(24 * 3600);

        // Prune entries older than 24h
        self.recent_spends.retain(|_, (timestamp, _)| {
            now.duration_since(*timestamp) < day
        });

        self.recent_spends.iter().map(|e| e.value().1).sum()
    }

    /// Verify hard limits prior to executing any trade (manual or auto)
    pub fn validate_trade(&self, mint: &str, amount_sol: f64) -> Result<()> {
        if self.is_killed() {
            bail!("Trade rejected: Global kill switch is currently ENGAGED.");
        }

        if amount_sol > self.settings.max_sol_per_trade {
            bail!(
                "Trade rejected: Amount {:.3} SOL exceeds hard cap of {:.3} SOL per trade.",
                amount_sol,
                self.settings.max_sol_per_trade
            );
        }

        let current_spend = self.get_rolling_24h_spend_sol();
        if current_spend + amount_sol > self.settings.max_daily_spend_sol {
            bail!(
                "Trade rejected: Daily spend limit reached. Current: {:.3} SOL, Max: {:.3} SOL.",
                current_spend,
                self.settings.max_daily_spend_sol
            );
        }

        if self.active_positions.len() >= self.settings.max_open_positions && !self.active_positions.contains_key(mint) {
            bail!(
                "Trade rejected: Max open positions limit reached ({}/{}).",
                self.active_positions.len(),
                self.settings.max_open_positions
            );
        }

        Ok(())
    }

    /// Record a completed spend
    pub fn record_spend(&self, trade_id: &str, mint: &str, amount_sol: f64) {
        self.recent_spends.insert(trade_id.to_string(), (Instant::now(), amount_sol));
        self.active_positions
            .entry(mint.to_string())
            .and_modify(|v| *v += amount_sol)
            .or_insert(amount_sol);
    }

    pub fn get_active_positions_count(&self) -> usize {
        self.active_positions.len()
    }
}
