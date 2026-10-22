use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SafetySettings {
    #[serde(default = "default_auto_buy_enabled")]
    pub auto_buy_enabled: bool,
    #[serde(default = "default_max_sol_per_trade")]
    pub max_sol_per_trade: f64,
    #[serde(default = "default_max_daily_spend_sol")]
    pub max_daily_spend_sol: f64,
    #[serde(default = "default_max_open_positions")]
    pub max_open_positions: usize,
    #[serde(default = "default_max_slippage_pct")]
    pub max_slippage_pct: f64,
    #[serde(default = "default_simulate_before_send")]
    pub simulate_before_send: bool,
}

fn default_auto_buy_enabled() -> bool { false }
fn default_max_sol_per_trade() -> f64 { 0.1 }
fn default_max_daily_spend_sol() -> f64 { 1.0 }
fn default_max_open_positions() -> usize { 5 }
fn default_max_slippage_pct() -> f64 { 2.0 }
fn default_simulate_before_send() -> bool { true }

impl Default for SafetySettings {
    fn default() -> Self {
        Self {
            auto_buy_enabled: default_auto_buy_enabled(),
            max_sol_per_trade: default_max_sol_per_trade(),
            max_daily_spend_sol: default_max_daily_spend_sol(),
            max_open_positions: default_max_open_positions(),
            max_slippage_pct: default_max_slippage_pct(),
            simulate_before_send: default_simulate_before_send(),
        }
    }
}
