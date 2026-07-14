use crate::POSITION_QUANTITY_TOLERANCE;

#[derive(Debug, Clone)]
pub struct PortfolioLaneView {
    pub lane_id: String,
    pub bot_id: String,
    pub account_id: String,
    pub symbol: String,
    pub budget_pct: f64,
    pub effective_position_quantity: f64,
    pub position_notional_usd: f64,
    pub daily_loss_pct_accumulated: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct AccountRiskSnapshot {
    pub open_positions: u32,
    pub daily_loss_pct: f64,
}

#[must_use]
pub fn account_risk_snapshot(lanes: &[PortfolioLaneView]) -> AccountRiskSnapshot {
    let mut open_positions = 0_u32;
    let mut daily_loss_pct = 0.0;

    for lane in lanes {
        if lane.effective_position_quantity > POSITION_QUANTITY_TOLERANCE {
            open_positions = open_positions.saturating_add(1);
        }
        daily_loss_pct += lane.daily_loss_pct_accumulated;
    }

    AccountRiskSnapshot {
        open_positions,
        daily_loss_pct,
    }
}
