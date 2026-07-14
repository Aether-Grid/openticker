mod activity;
mod bot_actions;
mod bots;
mod config;
mod connectors;
mod dashboard;
mod data_streams;
mod ledger;
mod platform;
mod risk;
mod ui;
mod views;

pub(crate) use activity::{
    list_events_handler, list_fills_handler, list_intents_handler, list_orders_handler,
    list_positions_handler, list_reconciliations_handler, list_risk_decisions_handler,
    list_signals_handler,
};
pub(crate) use bot_actions::{
    cancel_bot_open_orders_handler, close_bot_positions_handler, manual_bot_signal_handler,
    pause_bot_handler, reconcile_bot_handler, resume_bot_handler, simulate_bot_bar_handler,
    simulate_bot_trade_handler, start_bot_handler, stop_bot_handler, tick_bot_handler,
};
pub(crate) use bots::{
    bot_reconciliation_report_handler, get_bot_cycle_handler, get_bot_handler,
    get_bot_lanes_handler, list_bot_cycles_handler, list_bots_handler,
};
pub(crate) use config::{
    config_effective_handler, config_reload_handler, config_reload_status_handler,
};
pub(crate) use connectors::{connectors_matrix_handler, connectors_status_handler};
pub(crate) use dashboard::{bot_snapshot_handler, dashboard_snapshot_handler};
pub(crate) use data_streams::{
    list_data_stream_bars_handler, list_data_stream_history_handler, list_data_streams_handler,
};
pub(crate) use ledger::{
    ledger_accounts_handler, ledger_bots_handler, ledger_handler, ledger_lanes_handler,
};
pub(crate) use platform::{
    health_handler, metrics_handler, openapi_handler, ready_handler, service_status_handler,
};
pub(crate) use risk::{disable_kill_switch_handler, enable_kill_switch_handler};
pub(crate) use ui::{dashboard_handler, favicon_handler, ui_asset_handler};

use crate::constants::{BLOCKING_QUERY_TIMEOUT, MAX_QUERY_LIMIT};
use crate::state::ErrorResponse;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use openticker_runtime::ServiceError;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

pub(crate) fn unix_now_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

async fn run_blocking_query<T, F>(operation: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ServiceError> + Send + 'static,
{
    // Bound how long a handler waits on storage. Note that a timed-out
    // closure keeps running on the blocking pool until it returns on its
    // own; the timeout only frees the handler (and its client) to respond.
    let join_result = match tokio::time::timeout(
        BLOCKING_QUERY_TIMEOUT,
        tokio::task::spawn_blocking(operation),
    )
    .await
    {
        Ok(join_result) => join_result,
        Err(_elapsed) => {
            warn!(
                timeout_secs = BLOCKING_QUERY_TIMEOUT.as_secs(),
                "blocking query timed out; storage backend is unresponsive"
            );
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "query timed out; storage backend is unresponsive".to_owned(),
                }),
            )
                .into_response());
        }
    };
    match join_result {
        Ok(result) => result.map_err(|error| service_error_into_response(&error)),
        Err(error) => {
            error!(error = %error, "blocking query task panicked or was cancelled");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal error".to_owned(),
                }),
            )
                .into_response())
        }
    }
}

/// Clamps a caller-supplied `limit` query parameter to [`MAX_QUERY_LIMIT`],
/// falling back to `default` when absent. Every limit accepted from a request
/// must pass through this helper so no endpoint forwards an unbounded value
/// to storage.
fn clamped_limit(limit: Option<usize>, default: usize) -> usize {
    limit.unwrap_or(default).min(MAX_QUERY_LIMIT)
}

#[derive(Debug, Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

/// Stable, leak-free identifier for a [`ServiceError`] variant, suitable for
/// log fields where the full `Display` output would expose internals.
fn service_error_category(error: &ServiceError) -> &'static str {
    match error {
        ServiceError::InstanceNotFound(_) => "instance_not_found",
        ServiceError::InstanceDisabled(_) => "instance_disabled",
        ServiceError::ReconciliationRequired { .. } => "reconciliation_required",
        ServiceError::KillSwitchEnabled => "kill_switch_enabled",
        ServiceError::InvalidTransition { .. } => "invalid_transition",
        ServiceError::TradeSymbolMismatch { .. } => "trade_symbol_mismatch",
        ServiceError::SymbolSelectionRequired { .. } => "symbol_selection_required",
        ServiceError::SymbolNotConfigured { .. } => "symbol_not_configured",
        ServiceError::ConnectorNotReady { .. } => "connector_not_ready",
        ServiceError::DataConnectorUnavailable { .. } => "data_connector_unavailable",
        ServiceError::ExecutionConnectorUnavailable { .. } => "execution_connector_unavailable",
        ServiceError::InvalidConfiguration(_) => "invalid_configuration",
        ServiceError::LedgerInvariantViolation { .. } => "ledger_invariant_violation",
        ServiceError::Data(_) => "data",
        ServiceError::Execution(_) => "execution",
        ServiceError::Storage(_) => "storage",
        ServiceError::Json(_) => "json",
    }
}

fn service_error_into_response(error: &ServiceError) -> axum::response::Response {
    let status = match error {
        ServiceError::InstanceNotFound(_) => StatusCode::NOT_FOUND,
        ServiceError::InvalidConfiguration(_)
        | ServiceError::SymbolSelectionRequired { .. }
        | ServiceError::SymbolNotConfigured { .. }
        | ServiceError::TradeSymbolMismatch { .. }
        | ServiceError::Data(_)
        | ServiceError::Execution(_) => StatusCode::BAD_REQUEST,
        ServiceError::InstanceDisabled(_)
        | ServiceError::ReconciliationRequired { .. }
        | ServiceError::ConnectorNotReady { .. }
        | ServiceError::KillSwitchEnabled
        | ServiceError::InvalidTransition { .. } => StatusCode::CONFLICT,
        ServiceError::DataConnectorUnavailable { .. }
        | ServiceError::ExecutionConnectorUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        ServiceError::Storage(_)
        | ServiceError::Json(_)
        | ServiceError::LedgerInvariantViolation { .. } => StatusCode::INTERNAL_SERVER_ERROR,
    };

    // Server-side failures log only the error category at error/warn level —
    // the full `Display` chain can carry internal state such as file paths or
    // connector configuration. The complete error stays available at debug
    // level for operators who opt in.
    let category = service_error_category(error);
    if status == StatusCode::SERVICE_UNAVAILABLE {
        warn!(status = %status, category, "request failed due to unavailable dependency");
        debug!(status = %status, error = %error, "unavailable dependency error detail");
    } else if status.is_server_error() {
        error!(status = %status, category, "request failed with server error");
        debug!(status = %status, error = %error, "server error detail");
    } else {
        info!(status = %status, error = %error, "request rejected");
    }

    // 5xx responses carry only the stable category string — never the raw
    // error chain — to avoid leaking internal state (file paths, connector
    // configuration, etc.) to callers. 4xx responses keep their full
    // informative messages as intentional client feedback.
    let body_message = if status.is_server_error() {
        category.to_owned()
    } else {
        error.to_string()
    };

    (
        status,
        Json(ErrorResponse {
            error: body_message,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::clamped_limit;
    use crate::constants::MAX_QUERY_LIMIT;

    #[test]
    fn clamped_limit_applies_default_and_upper_bound() {
        assert_eq!(clamped_limit(None, 100), 100);
        assert_eq!(clamped_limit(Some(5), 100), 5);
        assert_eq!(clamped_limit(Some(MAX_QUERY_LIMIT), 100), MAX_QUERY_LIMIT);
        assert_eq!(clamped_limit(Some(999_999_999), 100), MAX_QUERY_LIMIT);
    }
}
