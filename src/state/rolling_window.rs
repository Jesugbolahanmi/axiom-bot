use chrono::{DateTime, Duration, Utc};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub timestamp: DateTime<Utc>,
    pub sol_amount: f64,
    pub usd_amount: f64,
}

#[derive(Debug, Clone, Default)]
pub struct TradeRollingWindow {
    trades: VecDeque<TradeRecord>,
    cumulative_volume_sol: f64,
    cumulative_volume_usd: f64,
}

impl TradeRollingWindow {
    pub fn new() -> Self {
        Self {
            trades: VecDeque::new(),
            cumulative_volume_sol: 0.0,
            cumulative_volume_usd: 0.0,
        }
    }

    pub fn record_trade(&mut self, sol_amount: f64, usd_amount: f64) {
        let now = Utc::now();
        self.cumulative_volume_sol += sol_amount;
        self.cumulative_volume_usd += usd_amount;

        self.trades.push_back(TradeRecord {
            timestamp: now,
            sol_amount,
            usd_amount,
        });

        self.prune(now);
    }

    /// Prune trades older than 5 minutes
    pub fn prune(&mut self, now: DateTime<Utc>) {
        let window_limit = now - Duration::minutes(5);
        while let Some(front) = self.trades.front() {
            if front.timestamp < window_limit {
                self.trades.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculate volume in the last 5 minutes
    pub fn volume_last_5m_usd(&self) -> f64 {
        let now = Utc::now();
        let window_limit = now - Duration::minutes(5);
        self.trades
            .iter()
            .filter(|t| t.timestamp >= window_limit)
            .map(|t| t.usd_amount)
            .sum()
    }

    pub fn volume_last_5m_sol(&self) -> f64 {
        let now = Utc::now();
        let window_limit = now - Duration::minutes(5);
        self.trades
            .iter()
            .filter(|t| t.timestamp >= window_limit)
            .map(|t| t.sol_amount)
            .sum()
    }

    pub fn total_volume_usd(&self) -> f64 {
        self.cumulative_volume_usd
    }

    pub fn total_volume_sol(&self) -> f64 {
        self.cumulative_volume_sol
    }
}
