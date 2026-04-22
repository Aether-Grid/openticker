use openticker_config::ConfigBundle;
use openticker_core::Timeframe;
use openticker_dataplane::{DataPlane, StreamStatus};
use openticker_runtime::{
    ConnectorRuntimeStatus, LedgerPortfolioSnapshot, ReconciliationCheck, Runtime,
    RuntimeQueryHandle, ServiceStatus,
};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct HttpState {
    pub(crate) runtime: Arc<RwLock<Runtime>>,
    pub(crate) query: Arc<RwLock<RuntimeQueryHandle>>,
    pub(crate) config_bundle: Arc<RwLock<Option<ConfigBundle>>>,
    pub(crate) config_dir: Option<PathBuf>,
    pub(crate) data_plane: Arc<DataPlane>,
}

impl HttpState {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        let query = runtime.query_handle();
        let data_plane = Arc::new(DataPlane::new(runtime.effective_streams_for_dataplane()));
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            query: Arc::new(RwLock::new(query)),
            config_bundle: Arc::new(RwLock::new(None)),
            config_dir: None,
            data_plane,
        }
    }

    #[must_use]
    pub fn with_config(runtime: Runtime, config_dir: PathBuf, bundle: ConfigBundle) -> Self {
        let query = runtime.query_handle();
        let data_plane = Arc::new(DataPlane::new(runtime.effective_streams_for_dataplane()));
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            query: Arc::new(RwLock::new(query)),
            config_bundle: Arc::new(RwLock::new(Some(bundle))),
            config_dir: Some(config_dir),
            data_plane,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadyResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ErrorResponse {
    pub(crate) error: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpBotPollingStatus {
    pub(crate) enabled: bool,
    pub(crate) interval_ms: u64,
    pub(crate) last_attempt_ms: Option<i64>,
    pub(crate) last_success_ms: Option<i64>,
    pub(crate) last_error: Option<String>,
    pub(crate) last_polled_bar_timestamp: Option<String>,
    pub(crate) last_polled_bar_close: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpBotPnlStatus {
    pub(crate) realized_usd: f64,
    pub(crate) unrealized_usd: Option<f64>,
    pub(crate) total_usd: Option<f64>,
    pub(crate) mark_price: Option<f64>,
    pub(crate) mark_timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpBotSummary {
    pub(crate) id: String,
    pub(crate) enabled: bool,
    pub(crate) market: openticker_core::MarketType,
    pub(crate) symbols: Vec<String>,
    pub(crate) symbol: String,
    pub(crate) timeframe: Timeframe,
    pub(crate) account: String,
    pub(crate) execution_mode: openticker_core::ExecutionMode,
    pub(crate) live_mode_active: bool,
    pub(crate) mode_banner: String,
    pub(crate) state: openticker_runtime::InstanceRuntimeState,
    pub(crate) position: openticker_runtime::InstancePositionStatus,
    pub(crate) pnl: HttpBotPnlStatus,
    pub(crate) open_symbol_count: usize,
    pub(crate) aggregate_position_notional_usd: f64,
    pub(crate) aggregate_realized_pnl_usd: f64,
    pub(crate) reconciliation_blocked: bool,
    pub(crate) reconciliation_by_symbol: Vec<openticker_runtime::SymbolReconciliationSummary>,
    pub(crate) warmup: openticker_runtime::InstanceWarmupStatus,
    pub(crate) warmup_ready_symbols: usize,
    pub(crate) polling: HttpBotPollingStatus,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpBotDetail {
    #[serde(flatten)]
    pub(crate) bot: HttpBotSummary,
    pub(crate) lanes: Vec<openticker_runtime::LaneSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HttpReconciliationReport {
    pub(crate) bot: HttpBotSummary,
    pub(crate) latest: Option<ReconciliationCheck>,
    pub(crate) lanes: Vec<ReconciliationCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardRiskDecisionsSnapshot {
    pub(crate) count: usize,
    pub(crate) items: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DashboardHomeSnapshot {
    pub(crate) status: ServiceStatus,
    pub(crate) instances: Vec<HttpBotSummary>,
    pub(crate) data_streams: Vec<StreamStatus>,
    pub(crate) signals: Value,
    pub(crate) intents: Value,
    pub(crate) risk: DashboardRiskDecisionsSnapshot,
    pub(crate) orders: Value,
    pub(crate) fills: Value,
    pub(crate) positions: Value,
    pub(crate) reconciliations: Value,
    pub(crate) events: Value,
    pub(crate) provider_events: Value,
    pub(crate) connectors_status: Vec<ConnectorRuntimeStatus>,
    pub(crate) connectors_matrix: Value,
    pub(crate) ledger: LedgerPortfolioSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DashboardBotSnapshot {
    pub(crate) detail: HttpBotDetail,
    pub(crate) report: HttpReconciliationReport,
    pub(crate) timeline: Value,
    pub(crate) orders: Value,
    pub(crate) fills: Value,
    pub(crate) positions: Value,
}
