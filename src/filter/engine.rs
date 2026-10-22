use tracing::debug;
use crate::config::thresholds::{FilterThresholds, RevivalThresholds};
use crate::models::token::TokenState;

#[derive(Debug, Clone)]
pub struct FilterResult {
    pub passed: bool,
    pub rejection_reason: Option<String>,
}

pub struct FilterEngine {
    thresholds: FilterThresholds,
    revival_thresholds: RevivalThresholds,
}

impl FilterEngine {
    pub fn new(thresholds: FilterThresholds, revival_thresholds: RevivalThresholds) -> Self {
        Self { thresholds, revival_thresholds }
    }

    /// Evaluate token against standard Early Coin criteria
    pub fn evaluate(&self, token: &TokenState) -> FilterResult {
        let age_mins = token.age_mins();

        // 1. Token Age check (5 min to 60 min)
        if age_mins < self.thresholds.min_age_mins as f64 {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Token age {:.1}m below minimum {}m",
                    age_mins, self.thresholds.min_age_mins
                )),
            };
        }

        if age_mins > self.thresholds.max_age_mins as f64 {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Token age {:.1}m exceeds maximum {}m",
                    age_mins, self.thresholds.max_age_mins
                )),
            };
        }

        // 2. Market Cap check ($10k to $35k)
        if token.market_cap_usd < self.thresholds.min_mcap_usd {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Market cap ${:.0} below minimum ${:.0}",
                    token.market_cap_usd, self.thresholds.min_mcap_usd
                )),
            };
        }

        if token.market_cap_usd > self.thresholds.max_mcap_usd {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Market cap ${:.0} exceeds maximum ${:.0}",
                    token.market_cap_usd, self.thresholds.max_mcap_usd
                )),
            };
        }

        // 3. Total Volume check (>= 1.0x Market Cap)
        let min_total_vol = token.market_cap_usd * self.thresholds.min_total_volume_mult;
        if token.total_volume_usd < min_total_vol {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Total volume ${:.0} below threshold ${:.0} ({:.1}x MC)",
                    token.total_volume_usd, min_total_vol, self.thresholds.min_total_volume_mult
                )),
            };
        }

        // 4. Last 5-minute volume check (>= 10% Market Cap)
        let min_5m_vol = token.market_cap_usd * self.thresholds.min_5m_volume_ratio;
        if token.rolling_5m_volume_usd < min_5m_vol {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "5m volume ${:.0} below threshold ${:.0} ({:.0}% MC)",
                    token.rolling_5m_volume_usd, min_5m_vol, self.thresholds.min_5m_volume_ratio * 100.0
                )),
            };
        }

        // 5. Liquidity check (>= 40% Market Cap)
        let min_liq = token.market_cap_usd * self.thresholds.min_liquidity_ratio;
        if token.liquidity_usd < min_liq {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Liquidity ${:.0} below threshold ${:.0} ({:.0}% MC)",
                    token.liquidity_usd, min_liq, self.thresholds.min_liquidity_ratio * 100.0
                )),
            };
        }

        // 6. Holder count check (>= 50)
        if token.holder_count < self.thresholds.min_holders {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Holders {} below minimum {}",
                    token.holder_count, self.thresholds.min_holders
                )),
            };
        }

        // 7. Dev holding check (<= 15%)
        if token.dev_holding_pct > self.thresholds.max_dev_holding_pct {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Dev holding {:.1}% exceeds maximum {:.1}%",
                    token.dev_holding_pct, self.thresholds.max_dev_holding_pct
                )),
            };
        }

        // 8. Top holder check (<= 15%, excluding curve/pool)
        if token.top_holder_pct > self.thresholds.max_top_holder_pct {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Top holder {:.1}% exceeds maximum {:.1}%",
                    token.top_holder_pct, self.thresholds.max_top_holder_pct
                )),
            };
        }

        // 9. Sniper + bundler share (<= 15%)
        if token.sniper_bundler_pct > self.thresholds.max_sniper_bundler_pct {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Sniper/bundler {:.1}% exceeds maximum {:.1}%",
                    token.sniper_bundler_pct, self.thresholds.max_sniper_bundler_pct
                )),
            };
        }

        debug!("Token {} PASSED all early filter criteria!", token.mint);
        FilterResult {
            passed: true,
            rejection_reason: None,
        }
    }

    /// Evaluate token against Token Revival criteria (Dormant CTO re-awakening)
    pub fn evaluate_revival(&self, token: &TokenState) -> FilterResult {
        if !self.revival_thresholds.enabled {
            return FilterResult {
                passed: false,
                rejection_reason: Some("Revival filter disabled".to_string()),
            };
        }

        let age_mins = token.age_mins();

        // 1. Must be older token (at least min_age_mins old, e.g. >= 60m = 1 hour old)
        if age_mins < self.revival_thresholds.min_age_mins as f64 {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Token age {:.1}m below revival minimum {}m",
                    age_mins, self.revival_thresholds.min_age_mins
                )),
            };
        }

        if age_mins > self.revival_thresholds.max_age_mins as f64 {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Token age {:.1}m exceeds revival tracking limit {}m",
                    age_mins, self.revival_thresholds.max_age_mins
                )),
            };
        }

        // 2. Maximum MCap ceiling check: must be a memecoin (<= $100,000), not a multi-million or billion dollar token
        if token.market_cap_usd > self.revival_thresholds.max_mcap_usd {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Current MCap ${:.0} exceeds revival ceiling ${:.0}",
                    token.market_cap_usd, self.revival_thresholds.max_mcap_usd
                )),
            };
        }

        // 3. Death / bottom MCap check: must have dropped to dormancy range (e.g. <= $6,000, 3k-5k zone)
        let bottom_mcap = if token.lowest_market_cap_usd > 0.0 {
            token.lowest_market_cap_usd
        } else {
            token.market_cap_usd
        };

        if bottom_mcap > self.revival_thresholds.max_death_mcap_usd {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Bottom MCap ${:.0} exceeds dormancy threshold ${:.0}",
                    bottom_mcap, self.revival_thresholds.max_death_mcap_usd
                )),
            };
        }

        // 3. Surge from death / bottom: >= +50%
        let surge_pct = if bottom_mcap > 0.0 {
            ((token.market_cap_usd - bottom_mcap) / bottom_mcap) * 100.0
        } else {
            0.0
        };

        if surge_pct < self.revival_thresholds.min_surge_from_death_pct {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Surge from death {:.1}% below required {:.0}%",
                    surge_pct, self.revival_thresholds.min_surge_from_death_pct
                )),
            };
        }

        // 4. Sudden buyer volume returning (last 5m)
        if token.rolling_5m_volume_usd < self.revival_thresholds.min_5m_volume_usd {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "5m volume ${:.0} below revival threshold ${:.0}",
                    token.rolling_5m_volume_usd, self.revival_thresholds.min_5m_volume_usd
                )),
            };
        }

        // 5. Dev dumped / CTO status (dev holding <= max_dev_holding_pct, e.g. <= 1%)
        if token.dev_holding_pct > self.revival_thresholds.max_dev_holding_pct {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Dev holding {:.1}% exceeds CTO max {:.1}%",
                    token.dev_holding_pct, self.revival_thresholds.max_dev_holding_pct
                )),
            };
        }

        // 6. Minimum holders
        if token.holder_count < self.revival_thresholds.min_holders {
            return FilterResult {
                passed: false,
                rejection_reason: Some(format!(
                    "Holders {} below revival minimum {}",
                    token.holder_count, self.revival_thresholds.min_holders
                )),
            };
        }

        debug!("Token {} PASSED Revival criteria!", token.mint);
        FilterResult {
            passed: true,
            rejection_reason: None,
        }
    }
}
