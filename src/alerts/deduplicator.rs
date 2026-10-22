use std::time::Duration;
use chrono::{DateTime, Utc};
use dashmap::DashMap;

pub struct AlertDeduplicator {
    cooldown: Duration,
    alerted_mints: DashMap<String, DateTime<Utc>>,
}

impl AlertDeduplicator {
    pub fn new(cooldown_mins: u64) -> Self {
        Self {
            cooldown: Duration::from_secs(cooldown_mins * 60),
            alerted_mints: DashMap::new(),
        }
    }

    /// Check if an alert can be sent for this token mint
    pub fn should_alert(&self, mint: &str) -> bool {
        let now = Utc::now();
        if let Some(entry) = self.alerted_mints.get(mint) {
            let elapsed = now.signed_duration_since(*entry).to_std().unwrap_or(Duration::ZERO);
            if elapsed < self.cooldown {
                return false;
            }
        }
        true
    }

    /// Record that an alert has been successfully dispatched
    pub fn record_alert(&self, mint: &str) {
        self.alerted_mints.insert(mint.to_string(), Utc::now());
    }

    /// Prune entries older than the cooldown
    pub fn prune(&self) {
        let now = Utc::now();
        let mut to_remove = Vec::new();
        for entry in self.alerted_mints.iter() {
            let elapsed = now.signed_duration_since(*entry.value()).to_std().unwrap_or(Duration::ZERO);
            if elapsed >= self.cooldown {
                to_remove.push(entry.key().clone());
            }
        }
        for mint in to_remove {
            self.alerted_mints.remove(&mint);
        }
    }
}
