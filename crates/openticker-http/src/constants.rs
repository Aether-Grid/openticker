use include_dir::{Dir, include_dir};
use std::time::Duration;

pub const HEALTH_PATH: &str = "/healthz";
pub const READY_PATH: &str = "/readyz";
pub const METRICS_PATH: &str = "/metrics";
pub const OPENAPI_PATH: &str = "/openapi.json";
pub const DASHBOARD_PATH: &str = "/dashboard";
pub const DASHBOARD_ACTIVITY_PATH: &str = "/activity";
pub const DASHBOARD_BOTS_PATH: &str = "/bots";
pub const DASHBOARD_BOT_DETAIL_PATH: &str = "/bots/{id}";
pub const DASHBOARD_CONFIG_PATH: &str = "/config";
pub const DASHBOARD_CONNECTORS_PATH: &str = "/connectors";
pub const DASHBOARD_CYCLES_PATH: &str = "/cycles";
pub const DASHBOARD_CYCLE_DETAIL_PATH: &str = "/cycles/{bot_id}/{trace_id}";
pub const DASHBOARD_FEEDS_PATH: &str = "/feeds";
pub const DASHBOARD_FEED_DETAIL_PATH: &str = "/feeds/{account}/{symbol}/{timeframe}";
pub const DASHBOARD_PROVIDERS_PATH: &str = "/providers";
pub const DASHBOARD_PORTFOLIO_PATH: &str = "/portfolio";
pub const DASHBOARD_SNAPSHOT_PATH: &str = "/v1/dashboard/snapshot";
pub const SERVICE_STATUS_PATH: &str = "/v1/service/status";
pub const LEDGER_PATH: &str = "/v1/ledger";
pub const LEDGER_ACCOUNTS_PATH: &str = "/v1/ledger/accounts";
pub const LEDGER_BOTS_PATH: &str = "/v1/ledger/bots";
pub const LEDGER_LANES_PATH: &str = "/v1/ledger/lanes";
pub const DASHBOARD_LEDGER_PATH: &str = "/api/ledger";
pub const CONFIG_RELOAD_PATH: &str = "/v1/config/reload";
pub const CONFIG_RELOAD_STATUS_PATH: &str = "/v1/config/reload-status";
pub const CONFIG_EFFECTIVE_PATH: &str = "/v1/config/effective";
pub const CONFIG_GLOBAL_PATH: &str = "/v1/config/global";
pub const CONFIG_BOTS_PATH: &str = "/v1/config/bots";
pub const CONFIG_BOT_PATH: &str = "/v1/config/bots/{id}";
pub const CONFIG_RISK_PROFILE_PATH: &str = "/v1/config/risk-profiles/{id}";
pub const CONFIG_ACCOUNT_PATH: &str = "/v1/config/accounts/{id}";
pub const CONNECTORS_MATRIX_PATH: &str = "/v1/connectors/matrix";
pub const CONNECTORS_STATUS_PATH: &str = "/v1/connectors/status";
pub const EVENTS_PATH: &str = "/v1/events";
pub const SIGNALS_PATH: &str = "/v1/signals";
pub const INTENTS_PATH: &str = "/v1/intents";
pub const RISK_DECISIONS_PATH: &str = "/v1/risk-decisions";
pub const ORDERS_PATH: &str = "/v1/orders";
pub const FILLS_PATH: &str = "/v1/fills";
pub const POSITIONS_PATH: &str = "/v1/positions";
pub const RECONCILIATIONS_PATH: &str = "/v1/reconciliations";
pub const DATA_STREAMS_PATH: &str = "/v1/data/streams";
pub const BOTS_PATH: &str = "/v1/bots";
pub const BOT_PATH: &str = "/v1/bots/{id}";
pub const BOT_LANES_PATH: &str = "/v1/bots/{id}/lanes";
pub const BOT_CYCLES_PATH: &str = "/v1/bots/{id}/cycles";
pub const BOT_CYCLE_DETAIL_PATH: &str = "/v1/bots/{id}/cycles/{trace_id}";
pub const BOT_SNAPSHOT_PATH: &str = "/v1/bots/{id}/snapshot";
pub const BOT_RECONCILIATION_REPORT_PATH: &str = "/v1/bots/{id}/reconciliation-report";
pub const BOT_SIMULATE_BAR_PATH: &str = "/v1/bots/{id}/simulate-bar";
pub const BOT_SIMULATE_TRADE_PATH: &str = "/v1/bots/{id}/simulate-trade";
pub const BOT_MANUAL_SIGNAL_PATH: &str = "/v1/bots/{id}/manual-signal";
pub const BOT_TICK_PATH: &str = "/v1/bots/{id}/tick";
pub const BOT_START_PATH: &str = "/v1/bots/{id}/start";
pub const BOT_STOP_PATH: &str = "/v1/bots/{id}/stop";
pub const BOT_PAUSE_PATH: &str = "/v1/bots/{id}/pause";
pub const BOT_RESUME_PATH: &str = "/v1/bots/{id}/resume";
pub const BOT_RECONCILE_PATH: &str = "/v1/bots/{id}/reconcile";
pub const BOT_CANCEL_OPEN_ORDERS_PATH: &str = "/v1/bots/{id}/cancel-open-orders";
pub const BOT_CLOSE_POSITIONS_PATH: &str = "/v1/bots/{id}/close-positions";

pub(crate) static UI_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/ui/.output/public");
pub(crate) const DASHBOARD_HTML: &str = include_str!("../ui/.output/public/index.html");

pub(crate) const STREAM_SPARKLINE_LIMIT: usize = 30;
pub(crate) const DEFAULT_STREAM_BARS_LIMIT: usize = 100;
pub(crate) const DASHBOARD_SNAPSHOT_DEFAULT_LIMIT: usize = 60;
pub(crate) const BOT_SNAPSHOT_TIMELINE_LIMIT: usize = 80;
pub(crate) const BOT_SNAPSHOT_ORDERS_LIMIT: usize = 500;
pub(crate) const BOT_SNAPSHOT_POSITIONS_LIMIT: usize = 120;

/// Hard upper bound applied to every caller-supplied `limit` query parameter.
pub(crate) const MAX_QUERY_LIMIT: usize = 1_000;
/// Maximum accepted request body size (1 MB) for all endpoints.
pub(crate) const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
/// Upper bound on how long a handler waits for a blocking storage query
/// before answering 503; a hung database must not stall handlers forever.
pub(crate) const BLOCKING_QUERY_TIMEOUT: Duration = Duration::from_secs(10);
