//! Externally-exposed Axum HTTP API and embedded dashboard server.
//!
//! # Authentication
//!
//! Bearer-token authentication is opt-in via the `OPENTICKER_API_TOKEN`
//! environment variable (see [`API_TOKEN_ENV`]), read at server startup by
//! [`serve`]. When set and non-empty, every endpoint requires
//! `Authorization: Bearer <token>` except health/readiness/metrics probes
//! and the embedded dashboard SPA assets; with a token configured, the
//! served dashboard cannot call the API itself. When unset or empty, the
//! API is unauthenticated (localhost development) and a warning is logged.

mod config_ops;
mod config_watcher;
mod config_write_handlers;
mod constants;
mod handlers;
mod openapi;
mod router;
mod runtime;
mod state;

pub use constants::{
    BOT_CANCEL_OPEN_ORDERS_PATH, BOT_CLOSE_POSITIONS_PATH, BOT_CYCLE_DETAIL_PATH, BOT_CYCLES_PATH,
    BOT_LANES_PATH, BOT_MANUAL_SIGNAL_PATH, BOT_PATH, BOT_PAUSE_PATH, BOT_RECONCILE_PATH,
    BOT_RECONCILIATION_REPORT_PATH, BOT_RESUME_PATH, BOT_SIMULATE_BAR_PATH,
    BOT_SIMULATE_TRADE_PATH, BOT_SNAPSHOT_PATH, BOT_START_PATH, BOT_STOP_PATH, BOT_TICK_PATH,
    BOTS_PATH, CONFIG_ACCOUNT_PATH, CONFIG_BOT_PATH, CONFIG_BOTS_PATH, CONFIG_EFFECTIVE_PATH,
    CONFIG_GLOBAL_PATH, CONFIG_RELOAD_PATH, CONFIG_RELOAD_STATUS_PATH, CONFIG_RISK_PROFILE_PATH,
    CONNECTORS_MATRIX_PATH, CONNECTORS_STATUS_PATH, DASHBOARD_ACTIVITY_PATH,
    DASHBOARD_BOT_DETAIL_PATH, DASHBOARD_BOTS_PATH, DASHBOARD_CONFIG_PATH,
    DASHBOARD_CONNECTORS_PATH, DASHBOARD_CYCLES_PATH, DASHBOARD_FEED_DETAIL_PATH,
    DASHBOARD_FEEDS_PATH, DASHBOARD_LEDGER_PATH, DASHBOARD_PATH, DASHBOARD_PORTFOLIO_PATH,
    DASHBOARD_PROVIDERS_PATH, DASHBOARD_SNAPSHOT_PATH, DATA_STREAMS_PATH, EVENTS_PATH, FILLS_PATH,
    HEALTH_PATH, INTENTS_PATH, LEDGER_ACCOUNTS_PATH, LEDGER_BOTS_PATH, LEDGER_LANES_PATH,
    LEDGER_PATH, METRICS_PATH, OPENAPI_PATH, ORDERS_PATH, POSITIONS_PATH, READY_PATH,
    RECONCILIATIONS_PATH, RISK_DECISIONS_PATH, SERVICE_STATUS_PATH, SIGNALS_PATH,
};
pub use router::{API_TOKEN_ENV, build_router, build_router_with_token};
pub use runtime::{load_http_state, serve};
pub use state::{HealthResponse, HttpState, ReadyResponse};

#[cfg(test)]
mod tests;
