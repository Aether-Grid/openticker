use crate::{
    ConnectionState, ConnectorResiliencePolicy, ConnectorResilienceState, ExecutionMode,
    IndicatorSignal, LaneRecoveryState, LaneRuntimeState, LedgerSnapshot, MarketType, OhlcvBar,
    RuntimeObservabilityStatus, SignalMetadata, SignalPhase, Timeframe, TradeIntent,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct InstanceSummary {
    pub id: String,
    pub enabled: bool,
    pub market: MarketType,
    pub symbols: Vec<String>,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub account: String,
    pub execution_mode: ExecutionMode,
    pub live_mode_active: bool,
    pub mode_banner: String,
    pub state: LaneRuntimeState,
    pub position: InstancePositionStatus,
    pub pnl: InstancePnlStatus,
    pub open_symbol_count: usize,
    pub aggregate_position_notional_usd: f64,
    pub aggregate_realized_pnl_usd: f64,
    pub reconciliation_blocked: bool,
    pub reconciliation_by_symbol: Vec<SymbolReconciliationSummary>,
    pub warmup: InstanceWarmupStatus,
    pub recovery: LaneRecoveryStatus,
    pub warmup_ready_symbols: usize,
    pub polling_enabled: bool,
    pub polling_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstancePositionStatus {
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstancePnlStatus {
    pub realized_usd: f64,
    pub realized_gross_usd: f64,
    pub realized_fees_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaneSummary {
    pub bot_id: String,
    pub symbol: String,
    pub state: LaneRuntimeState,
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub warmup: InstanceWarmupStatus,
    pub recovery: LaneRecoveryStatus,
    pub reconciliation_blocked: bool,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SymbolReconciliationSummary {
    pub symbol: String,
    pub reconciliation_blocked: bool,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceWarmupStatus {
    pub required_bars: usize,
    pub loaded_bars: usize,
    pub ready: bool,
    pub last_error: Option<String>,
    pub last_warmup_timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaneRecoveryStatus {
    pub state: LaneRecoveryState,
    pub trading_blocked_by_recovery: bool,
    pub started_at_ms: Option<i64>,
    pub target_timestamp: Option<String>,
    pub last_progress_timestamp: Option<String>,
    pub last_error: Option<String>,
    pub consecutive_no_progress_cycles: u32,
    pub last_recovered_at_timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub total_instances: usize,
    pub running_instances: usize,
    pub paused_instances: usize,
    pub stopped_instances: usize,
    pub reconciling_instances: usize,
    pub reconciliation_blocked_instances: usize,
    pub warmup_ready_instances: usize,
    pub warmup_pending_instances: usize,
    pub warmup_failed_instances: usize,
    pub kill_switch_active: bool,
    pub ready: bool,
    pub live_mode_active: bool,
    pub mode_banner: String,
    pub connector_resilience_windows_active: usize,
    pub observability: RuntimeObservabilityStatus,
    pub connector_statuses: Vec<ConnectorRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectorRuntimeStatus {
    pub account_id: String,
    pub kind: String,
    pub mode: ExecutionMode,
    pub live_mode_active: bool,
    pub mode_banner: String,
    pub state: ConnectionState,
    pub message: String,
    pub resilience_policy: ConnectorResiliencePolicy,
    pub resilience_state: ConnectorResilienceState,
}

pub type LedgerPortfolioSnapshot = LedgerSnapshot;

#[derive(Debug, Clone)]
pub struct PollAdvanceResult {
    pub outcomes: Vec<ProcessBarOutcome>,
    pub recorded_bars: Vec<OhlcvBar>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessBarOutcome {
    pub instance_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub phase: SignalPhase,
    pub signal: IndicatorSignal,
    pub signal_metadata: Option<SignalMetadata>,
    pub intent: TradeIntent,
    pub strategy_rationale: Option<String>,
    pub has_position: bool,
    pub risk: ProcessBarRisk,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessBarRisk {
    Allowed,
    Rejected { reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationCheck {
    pub id: i64,
    pub source: String,
    pub symbol: String,
    pub safe_to_trade: bool,
    pub local_open_orders: i64,
    pub connector_open_orders: i64,
    pub local_has_position: bool,
    pub connector_has_position: bool,
    pub reason: String,
    pub differences: Vec<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationReport {
    pub instance: InstanceSummary,
    pub latest: Option<ReconciliationCheck>,
    pub lanes: Vec<ReconciliationCheck>,
}
