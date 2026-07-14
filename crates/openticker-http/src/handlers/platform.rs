use crate::openapi::generated_openapi_spec;
use crate::state::{HealthResponse, HttpState, ReadyResponse};
use axum::Json;
use axum::extract::State;
use openticker_dataplane::DataPlaneMetricsSnapshot;
use openticker_runtime::ConnectorRuntimeStatus;
use serde_json::json;
use std::fmt::Write as _;

fn prometheus_label_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub(crate) async fn ready_handler(State(state): State<HttpState>) -> Json<ReadyResponse> {
    let runtime = state.runtime.read().await;
    let status = if runtime.is_ready() {
        "ready"
    } else {
        "not_ready"
    };
    Json(ReadyResponse { status })
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn metrics_handler(State(state): State<HttpState>) -> String {
    let (status, ledger) = {
        let runtime = state.runtime.read().await;
        (runtime.status(), runtime.ledger_snapshot())
    };
    let background_polling = state.data_plane.metrics_snapshot();
    let mut metrics = String::new();

    let _ = writeln!(
        metrics,
        "openticker_instances_total {}",
        status.total_instances
    );
    let _ = writeln!(
        metrics,
        "openticker_instances_running {}",
        status.running_instances
    );
    let _ = writeln!(
        metrics,
        "openticker_instances_paused {}",
        status.paused_instances
    );
    let _ = writeln!(
        metrics,
        "openticker_instances_reconciling {}",
        status.reconciling_instances
    );
    let _ = writeln!(
        metrics,
        "openticker_instances_reconciliation_blocked {}",
        status.reconciliation_blocked_instances
    );
    let _ = writeln!(
        metrics,
        "openticker_kill_switch_active {}",
        u8::from(status.kill_switch_active)
    );
    let _ = writeln!(
        metrics,
        "openticker_live_mode_active {}",
        u8::from(status.live_mode_active)
    );
    let _ = writeln!(
        metrics,
        "openticker_connector_resilience_windows_active {}",
        status.connector_resilience_windows_active
    );
    let _ = writeln!(
        metrics,
        "openticker_risk_rejects_total {}",
        status.observability.risk_rejects_total
    );
    let _ = writeln!(
        metrics,
        "openticker_ledger_reserve_attempts_total {}",
        status.observability.ledger_reserve_attempts_total
    );
    let _ = writeln!(
        metrics,
        "openticker_ledger_bot_rejects_total {}",
        status.observability.ledger_bot_rejects_total
    );
    let _ = writeln!(
        metrics,
        "openticker_ledger_account_rejects_total {}",
        status.observability.ledger_account_rejects_total
    );
    let _ = writeln!(
        metrics,
        "openticker_process_bar_latency_ms_last {}",
        status
            .observability
            .process_bar_latency_ms_last
            .unwrap_or(0)
    );
    let _ = writeln!(
        metrics,
        "openticker_process_bar_latency_ms_max {}",
        status.observability.process_bar_latency_ms_max
    );
    let _ = writeln!(
        metrics,
        "openticker_process_bar_latency_ms_avg {}",
        status
            .observability
            .process_bar_latency_ms_avg
            .unwrap_or(0.0)
    );
    let _ = writeln!(
        metrics,
        "openticker_execution_submit_latency_ms_last {}",
        status
            .observability
            .execution_submit_latency_ms_last
            .unwrap_or(0)
    );
    let _ = writeln!(
        metrics,
        "openticker_execution_submit_latency_ms_max {}",
        status.observability.execution_submit_latency_ms_max
    );
    let _ = writeln!(
        metrics,
        "openticker_execution_submit_latency_ms_avg {}",
        status
            .observability
            .execution_submit_latency_ms_avg
            .unwrap_or(0.0)
    );

    append_connector_metrics(&mut metrics, &status.connector_statuses);
    append_ledger_metrics(&mut metrics, &ledger);
    append_background_polling_metrics(&mut metrics, background_polling);

    metrics
}

fn append_connector_metrics(metrics: &mut String, connector_statuses: &[ConnectorRuntimeStatus]) {
    for connector in connector_statuses {
        let labels = format!(
            "account_id=\"{}\",kind=\"{}\"",
            prometheus_label_value(&connector.account_id),
            prometheus_label_value(&connector.kind)
        );
        let window_active = connector.resilience_state.next_reconnect_at_ms.is_some()
            || connector.resilience_state.throttled_until_ms.is_some();
        let _ = writeln!(
            metrics,
            "openticker_connector_resilience_window_active{{{labels}}} {}",
            u8::from(window_active)
        );
        if let Some(next_reconnect_at_ms) = connector.resilience_state.next_reconnect_at_ms {
            let _ = writeln!(
                metrics,
                "openticker_connector_next_reconnect_at_ms{{{labels}}} {next_reconnect_at_ms}"
            );
        }
        if let Some(throttled_until_ms) = connector.resilience_state.throttled_until_ms {
            let _ = writeln!(
                metrics,
                "openticker_connector_throttled_until_ms{{{labels}}} {throttled_until_ms}"
            );
        }
    }
}

fn append_ledger_metrics(
    metrics: &mut String,
    ledger: &openticker_runtime::LedgerPortfolioSnapshot,
) {
    for account in &ledger.accounts {
        let labels = format!("account_id=\"{}\"", prometheus_label_value(&account.id));
        let _ = writeln!(
            metrics,
            "openticker_ledger_effective_cap_usd{{{labels}}} {}",
            account.effective_cap_usd
        );
        let _ = writeln!(
            metrics,
            "openticker_ledger_total_committed_notional_usd{{{labels}}} {}",
            account.total_committed_notional_usd
        );
        let _ = writeln!(
            metrics,
            "openticker_ledger_tradeable_open_room_usd{{{labels}}} {}",
            account.tradeable_open_room_usd
        );
    }
}

fn append_background_polling_metrics(metrics: &mut String, polling: DataPlaneMetricsSnapshot) {
    let _ = writeln!(
        metrics,
        "openticker_background_poll_cycle_latency_ms_last {}",
        polling.cycle_latency.last_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_cycle_latency_ms_max {}",
        polling.cycle_latency.max_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_cycle_latency_ms_avg {}",
        polling.cycle_latency.avg_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_cycle_latency_samples {}",
        polling.cycle_latency.samples
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_connector_fetch_latency_ms_last {}",
        polling.connector_fetch_latency.last_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_connector_fetch_latency_ms_max {}",
        polling.connector_fetch_latency.max_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_connector_fetch_latency_ms_avg {}",
        polling.connector_fetch_latency.avg_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_connector_fetch_latency_samples {}",
        polling.connector_fetch_latency.samples
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_runtime_write_lock_wait_ms_last {}",
        polling.runtime_write_lock_wait.last_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_runtime_write_lock_wait_ms_max {}",
        polling.runtime_write_lock_wait.max_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_runtime_write_lock_wait_ms_avg {}",
        polling.runtime_write_lock_wait.avg_ms
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_runtime_write_lock_wait_samples {}",
        polling.runtime_write_lock_wait.samples
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_due_requests_last {}",
        polling.due_requests_last
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_due_requests_total {}",
        polling.due_requests_total
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_fetches_total {}",
        polling.fetches_total
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_completions_total {}",
        polling.completions_total
    );
    let _ = writeln!(
        metrics,
        "openticker_background_poll_completion_errors_total {}",
        polling.completion_errors_total
    );
}

pub(crate) async fn openapi_handler() -> Json<serde_json::Value> {
    Json(generated_openapi_spec().clone())
}

pub(crate) async fn service_status_handler(
    State(state): State<HttpState>,
) -> Json<serde_json::Value> {
    let status = {
        let runtime = state.runtime.read().await;
        runtime.status()
    };
    Json(json!(status))
}
