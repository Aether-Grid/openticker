use crate::warmup::InstanceWarmupState;
use openticker_config::{ExecutionConstraintsConfig, InstanceConfig};
use openticker_core::ExecutionMode;
use openticker_data::NormalizedBarUpdate;
use openticker_instance::{ConfiguredIndicatorRuntime, RuntimeStrategyEngine};
use openticker_ledger::InventoryState;
use openticker_risk::RiskLimits;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRuntimeState {
    Stopped,
    Running,
    Paused,
    Reconciling,
}

impl LaneRuntimeState {
    #[must_use]
    pub fn as_storage_value(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Reconciling => "reconciling",
        }
    }

    #[must_use]
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "stopped" => Some(Self::Stopped),
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "reconciling" => Some(Self::Reconciling),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRecoveryState {
    Healthy,
    CatchingUp,
    OutOfSync,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct LaneRuntime {
    pub config: InstanceConfig,
    pub lane_symbol: String,
    pub execution_mode: ExecutionMode,
    pub state: LaneRuntimeState,
    pub resume_after_startup_reconcile: bool,
    pub indicators: Vec<ConfiguredIndicatorRuntime>,
    pub strategy: RuntimeStrategyEngine,
    pub bar_builder: openticker_data::BarBuilder,
    pub risk_limits: RiskLimits,
    pub target_order_notional_usd: f64,
    pub inventory: InventoryState,
    pub has_position: bool,
    pub position_quantity: f64,
    pub position_notional_usd: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub daily_loss_pct_accumulated: f64,
    pub last_loss_reset_date: Option<chrono::NaiveDate>,
    pub cooldown_until_ms: Option<i64>,
    pub reconciliation_blocked: bool,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
    pub warmup: InstanceWarmupState,
    pub recovery_state: LaneRecoveryState,
    pub recovery_started_at_ms: Option<i64>,
    pub recovery_target_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub recovery_last_progress_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub recovery_last_error: Option<String>,
    pub recovery_consecutive_no_progress_cycles: u32,
    pub last_recovered_at_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_dispatched_bar_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_stream_update: Option<NormalizedBarUpdate>,
    pub connector_execution_constraints: Option<ExecutionConstraintsConfig>,
    pub connector_fractional_entry_supported: Option<bool>,
    pub connector_execution_constraints_initialized: bool,
}

#[must_use]
pub fn aggregate_bot_state(lanes: &[&LaneRuntime]) -> LaneRuntimeState {
    if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Reconciling))
    {
        LaneRuntimeState::Reconciling
    } else if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Running))
    {
        LaneRuntimeState::Running
    } else if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Paused))
    {
        LaneRuntimeState::Paused
    } else {
        LaneRuntimeState::Stopped
    }
}
