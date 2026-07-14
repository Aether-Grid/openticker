use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeEvent {
    pub id: i64,
    pub scope: String,
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventWrite {
    pub scope: String,
    pub entity_id: Option<String>,
    pub trace_id: Option<String>,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BotSnapshot {
    pub bot_id: String,
    pub state: String,
    pub execution_mode: String,
    pub enabled: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotSnapshotWrite {
    pub bot_id: String,
    pub state: String,
    pub execution_mode: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SignalRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub phase: String,
    pub signal: String,
    pub close: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub phase: String,
    pub signal: String,
    pub close: f64,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IntentRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub signal: String,
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_rationale: Option<String>,
    pub has_position_before: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntentWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub signal: String,
    pub intent: String,
    pub metadata_json: Option<String>,
    pub strategy_rationale: Option<String>,
    pub has_position_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RiskDecisionRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub intent: String,
    pub decision: String,
    pub reason: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskDecisionWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: String,
    pub intent: String,
    pub decision: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub intent: String,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub intent: String,
    pub status: String,
    pub price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FillRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_normalized_usd: Option<f64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FillWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub client_order_id: String,
    pub price: f64,
    pub quantity: f64,
    pub fee_asset: Option<String>,
    pub fee_amount: Option<f64>,
    pub fee_normalized_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionRecord {
    pub id: i64,
    pub bot_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bar_timestamp: Option<String>,
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionWrite {
    pub bot_id: String,
    pub symbol: String,
    pub trace_id: Option<String>,
    pub bar_timestamp: Option<String>,
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BotEventRecord {
    pub id: i64,
    pub bot_id: String,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotEventWrite {
    pub bot_id: String,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceEventRecord {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEventWrite {
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationRecord {
    pub id: i64,
    pub bot_id: String,
    pub source: String,
    pub symbol: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationWrite {
    pub bot_id: String,
    pub source: String,
    pub symbol: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CycleTraceRecord {
    pub id: i64,
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: String,
    pub trigger_kind: String,
    pub signal: String,
    pub intent: String,
    pub risk_decision: String,
    pub outcome: String,
    pub created_at_ms: i64,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleTraceWrite {
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: String,
    pub trigger_kind: String,
    pub signal: String,
    pub intent: String,
    pub risk_decision: String,
    pub outcome: String,
    pub payload_json: String,
}
