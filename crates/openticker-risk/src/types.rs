use openticker_core::TradeIntent;

#[derive(Debug, Clone, Copy)]
pub struct RiskLimits {
    pub max_daily_loss_pct: f64,
    pub max_open_positions: u32,
    pub max_order_notional_usd: f64,
    pub max_spread_bps: u32,
    pub max_slippage_bps: u32,
    pub stale_data_ms: u64,
    pub cooldown_after_reject_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct RiskContext {
    pub intent: TradeIntent,
    pub price: f64,
    pub quantity: f64,
    pub stale_data: bool,
    pub account_open_positions: u32,
    pub account_daily_loss_pct: f64,
    pub observed_spread_bps: u32,
    pub estimated_slippage_bps: u32,
    pub cooldown_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskDecision {
    Allow(TradeIntent),
    Reject { reason: &'static str },
}
