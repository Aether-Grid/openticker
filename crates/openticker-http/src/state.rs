use crate::config_ops::ConfigReloadStatus;
use openticker_config::ConfigBundle;
use openticker_core::Timeframe;
use openticker_dataplane::{DataPlane, StreamStatus};
use openticker_runtime::{
    ConnectorRuntimeStatus, LedgerPortfolioSnapshot, ReconciliationCheck, Runtime,
    RuntimeQueryHandle, ServiceStatus,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, RwLock};

#[derive(Debug, Clone)]
pub struct HttpState {
    pub(crate) runtime: Arc<RwLock<Runtime>>,
    pub(crate) query: Arc<RwLock<RuntimeQueryHandle>>,
    pub(crate) config_bundle: Arc<RwLock<Option<ConfigBundle>>>,
    pub(crate) config_dir: Option<PathBuf>,
    pub(crate) data_plane: Arc<DataPlane>,
    /// Serializes config reload/apply operations. Always acquire this BEFORE
    /// any `runtime`/`query`/`config_bundle` write locks.
    pub(crate) config_write_lock: Arc<Mutex<()>>,
    /// Ring buffer of the most recent reload attempts (newest at the back).
    pub(crate) reload_status: Arc<RwLock<VecDeque<ConfigReloadStatus>>>,
    /// Incremented every time a reload actually applies a new bundle.
    pub(crate) reload_generation: Arc<AtomicU64>,
    /// Resolved bots directory backing the managed config, when known.
    #[allow(dead_code)]
    pub(crate) bots_dir: Option<PathBuf>,
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
            config_write_lock: Arc::new(Mutex::new(())),
            reload_status: Arc::new(RwLock::new(VecDeque::new())),
            reload_generation: Arc::new(AtomicU64::new(0)),
            bots_dir: None,
        }
    }

    #[must_use]
    ///
    /// # Panics
    ///
    /// Panics if the provided runtime lock is contended while building the query handle.
    pub fn new_from_parts(runtime: Arc<RwLock<Runtime>>, data_plane: Arc<DataPlane>) -> Self {
        let query = runtime
            .try_read()
            .expect("HttpState::new_from_parts requires an uncontended runtime lock")
            .query_handle();
        Self {
            runtime,
            query: Arc::new(RwLock::new(query)),
            config_bundle: Arc::new(RwLock::new(None)),
            config_dir: None,
            data_plane,
            config_write_lock: Arc::new(Mutex::new(())),
            reload_status: Arc::new(RwLock::new(VecDeque::new())),
            reload_generation: Arc::new(AtomicU64::new(0)),
            bots_dir: None,
        }
    }

    #[must_use]
    pub fn with_config(runtime: Runtime, config_dir: PathBuf, bundle: ConfigBundle) -> Self {
        let query = runtime.query_handle();
        let data_plane = Arc::new(DataPlane::new(runtime.effective_streams_for_dataplane()));
        // Best-effort: callers pointing at a partial or synthetic config dir
        // still get a usable state with `bots_dir: None`.
        let bots_dir = openticker_config::load_sources_from_dir(&config_dir)
            .ok()
            .map(|sources| sources.bots_dir);
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            query: Arc::new(RwLock::new(query)),
            config_bundle: Arc::new(RwLock::new(Some(bundle))),
            config_dir: Some(config_dir),
            data_plane,
            config_write_lock: Arc::new(Mutex::new(())),
            reload_status: Arc::new(RwLock::new(VecDeque::new())),
            reload_generation: Arc::new(AtomicU64::new(0)),
            bots_dir,
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
