use crate::id::CycleTriggerKind;
use crate::summary::CycleRiskDecisionLabel;
use openticker_core::{IndicatorSignal, SignalMetadata, SignalPhase, TradeIntent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleTrigger {
    pub trigger_kind: CycleTriggerKind,
    pub phase: SignalPhase,
    pub bar_timestamp: String,
    pub close: f64,
    pub signal_source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalStep {
    pub signal: IndicatorSignal,
    pub close: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SignalMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentStep {
    pub signal: IndicatorSignal,
    pub intent: TradeIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_rationale: Option<String>,
    pub has_position_before: bool,
    pub order_quantity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_quantity_adjustment_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_ledger_outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetRoomContext {
    pub remaining_bot_usd: f64,
    pub remaining_account_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleDataDiagnostics {
    pub bar_timestamp_ms: i64,
    pub close_timestamp_ms: i64,
    pub stale_deadline_ms: i64,
    pub evaluated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskStep {
    pub intent: TradeIntent,
    pub decision: CycleRiskDecisionLabel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub stale_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_data_diagnostics: Option<StaleDataDiagnostics>,
    pub cooldown_active: bool,
    pub account_open_positions: u32,
    pub account_daily_loss_pct: f64,
    pub observed_spread_bps: u64,
    pub estimated_slippage_bps: u64,
    pub budget_room: BudgetRoomContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionOrderStep {
    pub client_order_id: String,
    pub intent: TradeIntent,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionFillStep {
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_amount: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_normalized_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ExecutionStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<ExecutionOrderStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<ExecutionFillStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionStep {
    pub has_position_before: bool,
    pub has_position_after: bool,
    pub quantity_after: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_price_after: Option<f64>,
    pub realized_pnl_usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationSnapshot {
    pub source: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReconciliationContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<ReconciliationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatedRecord {
    pub family: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelatedEvent {
    pub scope: String,
    pub kind: String,
    pub payload: Value,
}
