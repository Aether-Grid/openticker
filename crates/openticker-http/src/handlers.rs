use crate::config_ops::{ConfigApplyError, ReloadTrigger, reload_from_disk, violation_summary};
use crate::constants::{
    BOT_SNAPSHOT_ORDERS_LIMIT, BOT_SNAPSHOT_POSITIONS_LIMIT, BOT_SNAPSHOT_TIMELINE_LIMIT,
    DASHBOARD_HTML, DASHBOARD_SNAPSHOT_DEFAULT_LIMIT, DEFAULT_STREAM_BARS_LIMIT,
    STREAM_SPARKLINE_LIMIT, UI_DIST, generated_openapi_spec,
};
use crate::state::{
    DashboardBotSnapshot, DashboardHomeSnapshot, DashboardRiskDecisionsSnapshot, ErrorResponse,
    HealthResponse, HttpBotDetail, HttpBotPnlStatus, HttpBotPollingStatus, HttpBotSummary,
    HttpReconciliationReport, HttpState, ReadyResponse,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use openticker_connectors::connector_matrix;
use openticker_core::{IndicatorSignal, OhlcvBar, SignalPhase, Timeframe};
use openticker_data::NormalizedTrade;
use openticker_dataplane::{DataPlaneMetricsSnapshot, StreamKey, StreamStatus};
use openticker_runtime::{
    ConnectorRuntimeStatus, InstanceSummary as RuntimeInstanceSummary, ServiceError,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

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
    match tokio::task::spawn_blocking(operation).await {
        Ok(result) => result.map_err(|error| service_error_into_response(&error)),
        Err(error) => {
            let message = format!("blocking query task failed: {error}");
            error!(error = %message, "blocking query task panicked or was cancelled");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: message }),
            )
                .into_response())
        }
    }
}

fn prometheus_label_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) async fn dashboard_handler() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

pub(crate) async fn ui_asset_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = UI_DIST.get_file(path) {
        let mime = guess_ui_asset_mime(path);
        let cache_control = ui_asset_cache_header(path);
        (
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (axum::http::header::CACHE_CONTROL, cache_control),
            ],
            file.contents(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

pub(crate) async fn favicon_handler() -> Response {
    if let Some(file) = UI_DIST.get_file("favicon.ico") {
        (
            [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
            file.contents(),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn guess_ui_asset_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn ui_asset_cache_header(path: &str) -> &'static str {
    // Nuxt emits content-hashed files under _nuxt/ and _fonts/; these are safe to cache aggressively.
    if path.starts_with("_nuxt/") || path.starts_with("_fonts/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=300"
    }
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

pub(crate) async fn ledger_handler(State(state): State<HttpState>) -> Json<serde_json::Value> {
    let ledger = {
        let runtime = state.runtime.read().await;
        runtime.ledger_snapshot()
    };
    Json(json!(ledger))
}

pub(crate) async fn ledger_accounts_handler(
    State(state): State<HttpState>,
) -> Json<serde_json::Value> {
    let accounts = {
        let runtime = state.runtime.read().await;
        runtime.ledger_snapshot().accounts
    };
    Json(json!(accounts))
}

pub(crate) async fn ledger_bots_handler(State(state): State<HttpState>) -> Json<serde_json::Value> {
    let bots = {
        let runtime = state.runtime.read().await;
        runtime.ledger_snapshot().bots
    };
    Json(json!(bots))
}

pub(crate) async fn ledger_lanes_handler(
    State(state): State<HttpState>,
) -> Json<serde_json::Value> {
    let lanes = {
        let runtime = state.runtime.read().await;
        runtime.ledger_snapshot().lanes
    };
    Json(json!(lanes))
}

pub(crate) async fn list_data_streams_handler(
    State(state): State<HttpState>,
) -> Json<Vec<StreamStatus>> {
    Json(
        state
            .data_plane
            .snapshot_streams(unix_now_ms(), STREAM_SPARKLINE_LIMIT),
    )
}

pub(crate) async fn list_data_stream_bars_handler(
    State(state): State<HttpState>,
    Path((account, symbol, timeframe)): Path<(String, String, String)>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let timeframe = match Timeframe::from_str(&timeframe) {
        Ok(timeframe) => timeframe,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    };

    let stream_key = StreamKey {
        account_id: account,
        symbol,
        timeframe,
    };
    let limit = query.limit.unwrap_or(DEFAULT_STREAM_BARS_LIMIT);

    match state.data_plane.snapshot_bars(&stream_key, limit) {
        Ok(bars) => (StatusCode::OK, Json(json!(bars))).into_response(),
        Err(error) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

pub(crate) async fn list_data_stream_history_handler(
    State(state): State<HttpState>,
    Path((account, symbol, timeframe)): Path<(String, String, String)>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let timeframe = match Timeframe::from_str(&timeframe) {
        Ok(timeframe) => timeframe,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: error.to_string(),
                }),
            )
                .into_response();
        }
    };

    let stream_key = StreamKey {
        account_id: account,
        symbol,
        timeframe,
    };
    let limit = query.limit.unwrap_or(DEFAULT_STREAM_BARS_LIMIT).max(1);

    if let Err(error) = state.data_plane.snapshot_bars(&stream_key, 1) {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: error.to_string(),
            }),
        )
            .into_response();
    }

    let runtime = state.runtime.read().await;
    match runtime.stream_history(&stream_key, limit) {
        Ok(bars) => (
            StatusCode::OK,
            Json(json!({
                "source": "connector_history",
                "bars": bars,
            })),
        )
            .into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn config_effective_handler(State(state): State<HttpState>) -> impl IntoResponse {
    let config_bundle = state.config_bundle.read().await;
    let Some(bundle) = config_bundle.as_ref() else {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error: "service was started without a managed config bundle".to_owned(),
            }),
        )
            .into_response();
    };

    (StatusCode::OK, Json(json!(bundle.effective_config()))).into_response()
}

pub(crate) async fn connectors_matrix_handler() -> Json<serde_json::Value> {
    Json(json!(connector_matrix()))
}

pub(crate) async fn connectors_status_handler(
    State(state): State<HttpState>,
) -> Json<serde_json::Value> {
    let statuses = {
        let runtime = state.runtime.read().await;
        runtime.connector_statuses()
    };
    Json(json!(statuses))
}

pub(crate) async fn config_reload_handler(State(state): State<HttpState>) -> impl IntoResponse {
    match reload_from_disk(&state, ReloadTrigger::ManualApi).await {
        Ok(reloaded) => {
            let runtime = state.runtime.read().await;
            (
                StatusCode::OK,
                Json(json!({
                    "status": "reloaded",
                    "reloaded": reloaded,
                    "service": runtime.status(),
                })),
            )
                .into_response()
        }
        Err(ConfigApplyError::NotManaged) => (
            StatusCode::NOT_IMPLEMENTED,
            Json(ErrorResponse {
                error: "service was started without a reloadable config directory".to_owned(),
            }),
        )
            .into_response(),
        Err(ConfigApplyError::Load(error)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: format!("config reload failed: {error}"),
            }),
        )
            .into_response(),
        Err(ConfigApplyError::RuntimeBuild(error)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: format!("config reload failed: {error}"),
            }),
        )
            .into_response(),
        Err(ConfigApplyError::ChangeSet(violations)) => (
            StatusCode::CONFLICT,
            Json(json!({
                "error": format!("config reload failed: {}", violation_summary(&violations)),
                "violations": violations,
            })),
        )
            .into_response(),
    }
}

pub(crate) async fn config_reload_status_handler(
    State(state): State<HttpState>,
) -> Json<serde_json::Value> {
    let generation = state
        .reload_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let history = state.reload_status.read().await;
    let newest_first = history.iter().rev().cloned().collect::<Vec<_>>();
    Json(json!({
        "generation": generation,
        "last": newest_first.first(),
        "history": newest_first,
    }))
}

fn stream_status_map(streams: Vec<StreamStatus>) -> HashMap<String, StreamStatus> {
    streams
        .into_iter()
        .map(|stream| {
            (
                stream_key_id(
                    &stream.key.account_id,
                    &stream.key.symbol,
                    stream.key.timeframe,
                ),
                stream,
            )
        })
        .collect()
}

fn stream_key_id(account_id: &str, symbol: &str, timeframe: Timeframe) -> String {
    format!("{account_id}/{symbol}/{timeframe}")
}

#[allow(clippy::too_many_lines)]
fn bot_summary_view(
    summary: RuntimeInstanceSummary,
    stream_statuses: &HashMap<String, StreamStatus>,
    lane_summaries: Option<&[openticker_runtime::LaneSummary]>,
) -> HttpBotSummary {
    let polling_status = stream_statuses.get(&stream_key_id(
        &summary.account,
        &summary.symbol,
        summary.timeframe,
    ));
    let summary_mark_price = polling_status
        .and_then(|status| status.latest_bar.as_ref())
        .map(|bar| bar.close);
    let summary_mark_timestamp = polling_status
        .and_then(|status| status.latest_bar.as_ref())
        .map(|bar| bar.timestamp.to_rfc3339());
    let mut mark_price = summary_mark_price;
    let mut mark_timestamp = summary_mark_timestamp;

    let unrealized_usd = if let Some(lanes) = lane_summaries {
        let open_lanes = lanes
            .iter()
            .filter(|lane| {
                lane.has_position && lane.quantity.is_finite() && lane.quantity > f64::EPSILON
            })
            .collect::<Vec<_>>();

        if open_lanes.is_empty() {
            Some(0.0)
        } else {
            let mut lane_marks = Vec::with_capacity(open_lanes.len());
            let mut total_unrealized_usd = 0.0;
            let mut missing_lane_mark = false;

            for lane in open_lanes {
                let lane_entry_price = lane.entry_price.filter(|price| price.is_finite());
                let lane_mark = stream_statuses
                    .get(&stream_key_id(
                        &summary.account,
                        &lane.symbol,
                        summary.timeframe,
                    ))
                    .and_then(|status| status.latest_bar.as_ref())
                    .map(|bar| (bar.close, bar.timestamp.to_rfc3339()));

                match (lane_entry_price, lane_mark) {
                    (Some(entry_price), Some((mark, timestamp))) if mark.is_finite() => {
                        total_unrealized_usd += (mark - entry_price) * lane.quantity;
                        lane_marks.push((mark, timestamp));
                    }
                    _ => {
                        missing_lane_mark = true;
                        break;
                    }
                }
            }

            if missing_lane_mark {
                mark_price = None;
                mark_timestamp = None;
                None
            } else {
                if lane_marks.len() == 1 {
                    mark_price = Some(lane_marks[0].0);
                    mark_timestamp = Some(lane_marks[0].1.clone());
                } else {
                    mark_price = None;
                    mark_timestamp = None;
                }

                Some(total_unrealized_usd)
            }
        }
    } else if summary.position.has_position
        && summary.position.quantity > f64::EPSILON
        && summary.position.entry_price.is_some()
    {
        match (summary.position.entry_price, summary_mark_price) {
            (Some(entry_price), Some(mark)) if entry_price.is_finite() && mark.is_finite() => {
                Some((mark - entry_price) * summary.position.quantity)
            }
            _ => None,
        }
    } else {
        Some(0.0)
    };
    let total_usd = unrealized_usd.map(|unrealized_usd| summary.pnl.realized_usd + unrealized_usd);

    HttpBotSummary {
        id: summary.id,
        enabled: summary.enabled,
        market: summary.market,
        symbols: summary.symbols,
        symbol: summary.symbol,
        timeframe: summary.timeframe,
        account: summary.account,
        execution_mode: summary.execution_mode,
        live_mode_active: summary.live_mode_active,
        mode_banner: summary.mode_banner,
        state: summary.state,
        position: summary.position,
        pnl: HttpBotPnlStatus {
            realized_usd: summary.pnl.realized_usd,
            unrealized_usd,
            total_usd,
            mark_price,
            mark_timestamp,
        },
        open_symbol_count: summary.open_symbol_count,
        aggregate_position_notional_usd: summary.aggregate_position_notional_usd,
        aggregate_realized_pnl_usd: summary.aggregate_realized_pnl_usd,
        reconciliation_blocked: summary.reconciliation_blocked,
        reconciliation_by_symbol: summary.reconciliation_by_symbol,
        warmup: summary.warmup,
        warmup_ready_symbols: summary.warmup_ready_symbols,
        polling: HttpBotPollingStatus {
            enabled: summary.polling_enabled,
            interval_ms: summary.polling_interval_ms,
            last_attempt_ms: polling_status.and_then(|status| status.last_attempt_ms),
            last_success_ms: polling_status.and_then(|status| status.last_success_ms),
            last_error: polling_status.and_then(|status| status.last_error.clone()),
            last_polled_bar_timestamp: polling_status
                .and_then(|status| status.latest_bar.as_ref())
                .map(|bar| bar.timestamp.to_rfc3339()),
            last_polled_bar_close: polling_status
                .and_then(|status| status.latest_bar.as_ref())
                .map(|bar| bar.close),
        },
    }
}

fn bot_detail_view(
    summary: RuntimeInstanceSummary,
    lanes: Vec<openticker_runtime::LaneSummary>,
    stream_statuses: &HashMap<String, StreamStatus>,
) -> HttpBotDetail {
    let bot = bot_summary_view(summary, stream_statuses, Some(lanes.as_slice()));
    HttpBotDetail { bot, lanes }
}

fn reconciliation_report_view(
    report: openticker_runtime::ReconciliationReport,
    lane_summaries: &[openticker_runtime::LaneSummary],
    stream_statuses: &HashMap<String, StreamStatus>,
) -> HttpReconciliationReport {
    HttpReconciliationReport {
        bot: bot_summary_view(report.instance, stream_statuses, Some(lane_summaries)),
        latest: report.latest,
        lanes: report.lanes,
    }
}

async fn dashboard_home_snapshot(
    state: &HttpState,
    limit: usize,
) -> Result<DashboardHomeSnapshot, ServiceError> {
    let query = state.query.read().await.clone();
    let stream_statuses = stream_status_map(state.data_plane.snapshot_streams(unix_now_ms(), 1));
    let data_streams = state
        .data_plane
        .snapshot_streams(unix_now_ms(), STREAM_SPARKLINE_LIMIT);

    let (status, summaries, lane_summaries_by_bot, ledger) = {
        let runtime = state.runtime.read().await;
        let status = runtime.status();
        let summaries = runtime.list_instances();
        let lane_summaries_by_bot = summaries
            .iter()
            .filter_map(|summary| {
                runtime
                    .lane_summaries_for_bot(summary.id.as_str())
                    .ok()
                    .map(|lanes| (summary.id.clone(), lanes))
            })
            .collect::<HashMap<_, _>>();
        let ledger = runtime.ledger_snapshot();
        (status, summaries, lane_summaries_by_bot, ledger)
    };

    let instances = summaries
        .into_iter()
        .map(|summary| {
            let lane_summaries = lane_summaries_by_bot
                .get(summary.id.as_str())
                .map(Vec::as_slice);
            bot_summary_view(summary, &stream_statuses, lane_summaries)
        })
        .collect::<Vec<_>>();

    let signals = query.snapshot_recent_signals(limit);
    let intents = query.snapshot_recent_intents(limit);
    let risk_items = query.snapshot_recent_risk_decisions(limit);
    let orders = query.snapshot_recent_orders(limit);
    let fills = query.snapshot_recent_fills(limit);
    let positions = query.snapshot_recent_positions(limit);
    let reconciliations = query.snapshot_recent_reconciliations(limit);
    let events = query.snapshot_recent_events(limit);
    let provider_events = query.snapshot_recent_events_by_scope("provider", limit);
    let connectors_status = status.connector_statuses.clone();

    Ok(DashboardHomeSnapshot {
        status,
        instances,
        data_streams,
        signals: json!(signals),
        intents: json!(intents),
        risk: DashboardRiskDecisionsSnapshot {
            count: risk_items.len(),
            items: json!(risk_items),
        },
        orders: json!(orders),
        fills: json!(fills),
        positions: json!(positions),
        reconciliations: json!(reconciliations),
        events: json!(events),
        provider_events: json!(provider_events),
        connectors_status,
        connectors_matrix: json!(connector_matrix()),
        ledger,
    })
}

async fn dashboard_bot_snapshot(
    state: &HttpState,
    instance_id: &str,
) -> Result<DashboardBotSnapshot, Response> {
    let query = state.query.read().await.clone();
    let (summary, lanes) = {
        let runtime = state.runtime.read().await;
        (
            runtime
                .get_instance(instance_id)
                .map_err(|error| service_error_into_response(&error))?,
            runtime
                .lane_summaries_for_bot(instance_id)
                .map_err(|error| service_error_into_response(&error))?,
        )
    };
    let stream_statuses = stream_status_map(state.data_plane.snapshot_streams(unix_now_ms(), 1));
    let detail = bot_detail_view(summary.clone(), lanes.clone(), &stream_statuses);
    let reconciliation_query = query.clone();
    let lanes_for_query = lanes.clone();
    let report = run_blocking_query(move || {
        reconciliation_query.snapshot_reconciliation_report(summary, lanes_for_query.as_slice())
    })
    .await?;

    Ok(DashboardBotSnapshot {
        detail,
        report: reconciliation_report_view(report, lanes.as_slice(), &stream_statuses),
        timeline: json!(
            query.snapshot_recent_events_for_entity(instance_id, BOT_SNAPSHOT_TIMELINE_LIMIT,)
        ),
        orders: json!(
            query.snapshot_recent_orders_for_bot(instance_id, BOT_SNAPSHOT_ORDERS_LIMIT,)
        ),
        fills: json!(query.snapshot_recent_fills_for_bot(instance_id, BOT_SNAPSHOT_ORDERS_LIMIT,)),
        positions: json!(
            query.snapshot_recent_positions_for_bot(instance_id, BOT_SNAPSHOT_POSITIONS_LIMIT,)
        ),
    })
}

pub(crate) async fn list_bots_handler(State(state): State<HttpState>) -> Json<serde_json::Value> {
    let (summaries, lane_summaries_by_bot) = {
        let runtime = state.runtime.read().await;
        let summaries = runtime.list_instances();
        let lane_summaries_by_bot = summaries
            .iter()
            .filter_map(|summary| {
                runtime
                    .lane_summaries_for_bot(summary.id.as_str())
                    .ok()
                    .map(|lanes| (summary.id.clone(), lanes))
            })
            .collect::<HashMap<_, _>>();
        (summaries, lane_summaries_by_bot)
    };
    // The runtime read guard is intentionally dropped before we snapshot dataplane state.
    let stream_statuses = stream_status_map(state.data_plane.snapshot_streams(unix_now_ms(), 1));
    let instances = summaries
        .into_iter()
        .map(|summary| {
            let lane_summaries = lane_summaries_by_bot
                .get(summary.id.as_str())
                .map(Vec::as_slice);
            bot_summary_view(summary, &stream_statuses, lane_summaries)
        })
        .collect::<Vec<_>>();
    Json(json!(instances))
}

pub(crate) async fn dashboard_snapshot_handler(
    State(state): State<HttpState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = query
        .limit
        .unwrap_or(DASHBOARD_SNAPSHOT_DEFAULT_LIMIT)
        .min(1_000);
    match dashboard_home_snapshot(&state, limit).await {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn bot_snapshot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    match dashboard_bot_snapshot(&state, &instance_id).await {
        Ok(snapshot) => (StatusCode::OK, Json(json!(snapshot))).into_response(),
        Err(response) => response,
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct SimulateBarRequest {
    bar: OhlcvBar,
    phase: Option<SignalPhase>,
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SimulateTradeRequest {
    trade: NormalizedTrade,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ManualSignalRequest {
    signal: IndicatorSignal,
    price: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TickQuery {
    symbol: Option<String>,
}

pub(crate) async fn simulate_bot_bar_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
    Json(request): Json<SimulateBarRequest>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    let phase = request.phase.unwrap_or(SignalPhase::Confirmed);
    let result = if let Some(symbol) = request.symbol.as_deref() {
        runtime.process_bar_for_symbol(&instance_id, symbol, &request.bar, phase)
    } else {
        runtime.process_bar(&instance_id, &request.bar, phase)
    };
    match result {
        Ok(outcome) => (StatusCode::OK, Json(json!(outcome))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn simulate_bot_trade_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
    Json(request): Json<SimulateTradeRequest>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.process_trade(&instance_id, &request.trade) {
        Ok(outcomes) => (StatusCode::OK, Json(json!(outcomes))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn manual_bot_signal_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
    Json(request): Json<ManualSignalRequest>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    let result = if let Some(symbol) = request.symbol.as_deref() {
        runtime.process_manual_signal_for_symbol(
            &instance_id,
            symbol,
            request.signal,
            request.price,
            request.timestamp,
        )
    } else {
        runtime.process_manual_signal(
            &instance_id,
            request.signal,
            request.price,
            request.timestamp,
        )
    };
    match result {
        Ok(outcome) => (StatusCode::OK, Json(json!(outcome))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn tick_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
    Query(query): Query<TickQuery>,
) -> impl IntoResponse {
    let now_ms = unix_now_ms();

    let stream_key = {
        let runtime = state.runtime.read().await;
        let result = if let Some(symbol) = query.symbol.as_deref() {
            runtime.stream_key_for_symbol(&instance_id, symbol)
        } else {
            runtime.stream_key_for_instance(&instance_id)
        };
        match result {
            Ok(stream_key) => stream_key,
            Err(error) => return service_error_into_response(&error),
        }
    };

    let _ = state
        .data_plane
        .record_manual_poll_attempt(&stream_key, now_ms);

    let advanced = {
        let mut runtime = state.runtime.write().await;
        if let Some(symbol) = query.symbol.as_deref() {
            runtime.poll_instance_symbol_once_detailed(&instance_id, symbol)
        } else {
            runtime.poll_instance_once_detailed(&instance_id)
        }
    };

    match advanced {
        Ok(advance) => {
            for bar in advance.recorded_bars {
                if let Err(error) = state
                    .data_plane
                    .record_fetched_bar(&stream_key, now_ms, bar)
                {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: error.to_string(),
                        }),
                    )
                        .into_response();
                }
            }
            (StatusCode::OK, Json(json!(advance.outcomes))).into_response()
        }
        Err(error) => {
            let _ = state.data_plane.record_fetch_error(&stream_key, &error);
            service_error_into_response(&error)
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    limit: Option<usize>,
    scope: Option<String>,
    entity_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BotLimitQuery {
    limit: Option<usize>,
    bot_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReconciliationsQuery {
    limit: Option<usize>,
    bot_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BotCyclesQuery {
    limit: Option<usize>,
    symbol: Option<String>,
    phase: Option<String>,
    outcome: Option<String>,
    bar_timestamp: Option<String>,
}

pub(crate) async fn list_events_handler(
    State(state): State<HttpState>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    let result = run_blocking_query(move || match (query.scope, query.entity_id) {
        (Some(scope), Some(entity_id)) => {
            query_handle.recent_events_by_scope_and_entity(&scope, &entity_id, limit)
        }
        (Some(scope), None) => query_handle.recent_events_by_scope(&scope, limit),
        (None, Some(entity_id)) => query_handle.recent_events_for_entity(&entity_id, limit),
        (None, None) => query_handle.recent_events(limit),
    })
    .await;

    match result {
        Ok(events) => (StatusCode::OK, Json(json!(events))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_signals_handler(
    State(state): State<HttpState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    match run_blocking_query(move || query_handle.recent_signals(limit)).await {
        Ok(signals) => (StatusCode::OK, Json(json!(signals))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_intents_handler(
    State(state): State<HttpState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    match run_blocking_query(move || query_handle.recent_intents(limit)).await {
        Ok(intents) => (StatusCode::OK, Json(json!(intents))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_risk_decisions_handler(
    State(state): State<HttpState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    match run_blocking_query(move || query_handle.recent_risk_decisions(limit)).await {
        Ok(decisions) => {
            let count = decisions.len();
            (
                StatusCode::OK,
                Json(json!({ "count": count, "items": decisions })),
            )
                .into_response()
        }
        Err(response) => response,
    }
}

pub(crate) async fn list_orders_handler(
    State(state): State<HttpState>,
    Query(query): Query<BotLimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    let result = run_blocking_query(move || {
        if let Some(bot_id) = query.bot_id {
            query_handle.recent_orders_for_bot(&bot_id, limit)
        } else {
            query_handle.recent_orders(limit)
        }
    })
    .await;
    match result {
        Ok(orders) => (StatusCode::OK, Json(json!(orders))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_fills_handler(
    State(state): State<HttpState>,
    Query(query): Query<BotLimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    let result = run_blocking_query(move || {
        if let Some(bot_id) = query.bot_id {
            query_handle.recent_fills_for_bot(&bot_id, limit)
        } else {
            query_handle.recent_fills(limit)
        }
    })
    .await;
    match result {
        Ok(fills) => (StatusCode::OK, Json(json!(fills))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_positions_handler(
    State(state): State<HttpState>,
    Query(query): Query<BotLimitQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    let result = run_blocking_query(move || {
        if let Some(bot_id) = query.bot_id {
            query_handle.recent_positions_for_bot(&bot_id, limit)
        } else {
            query_handle.recent_positions(limit)
        }
    })
    .await;
    match result {
        Ok(positions) => (StatusCode::OK, Json(json!(positions))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn list_reconciliations_handler(
    State(state): State<HttpState>,
    Query(query): Query<ReconciliationsQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(100).min(1_000);
    let bot_id = query.bot_id;
    if let Some(bot_id) = bot_id.as_deref() {
        let validation = {
            let runtime = state.runtime.read().await;
            runtime.get_instance(bot_id)
        };
        if let Err(error) = validation {
            return service_error_into_response(&error);
        }
    }

    let result = run_blocking_query(move || {
        if let Some(bot_id) = bot_id {
            query_handle.recent_reconciliations_for_bot(&bot_id, limit)
        } else {
            query_handle.recent_reconciliations(limit)
        }
    })
    .await;

    match result {
        Ok(records) => (StatusCode::OK, Json(json!(records))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn get_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let (summary, lanes) = {
        let runtime = state.runtime.read().await;
        (
            runtime.get_instance(&instance_id),
            runtime.lane_summaries_for_bot(&instance_id),
        )
    };
    let stream_statuses = stream_status_map(state.data_plane.snapshot_streams(unix_now_ms(), 1));
    match (summary, lanes) {
        (Ok(summary), Ok(lanes)) => (
            StatusCode::OK,
            Json(json!(bot_detail_view(summary, lanes, &stream_statuses))),
        )
            .into_response(),
        (Err(error), _) | (_, Err(error)) => service_error_into_response(&error),
    }
}

pub(crate) async fn get_bot_lanes_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    match runtime.lane_summaries_for_bot(&instance_id) {
        Ok(lanes) => (StatusCode::OK, Json(json!(lanes))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn list_bot_cycles_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
    Query(query): Query<BotCyclesQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = query.limit.unwrap_or(50).min(1_000);
    match run_blocking_query(move || {
        query_handle.recent_cycle_traces_for_bot(
            &instance_id,
            query.symbol.as_deref(),
            query.phase.as_deref(),
            query.outcome.as_deref(),
            query.bar_timestamp.as_deref(),
            limit,
        )
    })
    .await
    {
        Ok(cycles) => (StatusCode::OK, Json(json!(cycles))).into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn get_bot_cycle_handler(
    State(state): State<HttpState>,
    Path((instance_id, trace_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let cycle_instance_id = instance_id.clone();
    let cycle_trace_id = trace_id.clone();
    match run_blocking_query(move || {
        query_handle.cycle_trace_for_bot(&cycle_instance_id, &cycle_trace_id)
    })
    .await
    {
        Ok(Some(trace)) => (StatusCode::OK, Json(json!(trace))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("cycle trace `{trace_id}` does not exist for bot `{instance_id}`"),
            }),
        )
            .into_response(),
        Err(response) => response,
    }
}

pub(crate) async fn bot_reconciliation_report_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let (summary, lane_summaries) = {
        let runtime = state.runtime.read().await;
        let summary = runtime.get_instance(&instance_id);
        let lane_summaries = match summary.as_ref() {
            Ok(_) => runtime.lane_summaries_for_bot(&instance_id).ok(),
            Err(_) => None,
        };
        (summary, lane_summaries)
    };
    let report = match (summary, lane_summaries) {
        (Ok(summary), Some(lane_summaries)) => {
            let lane_summaries_for_query = lane_summaries.clone();
            match run_blocking_query(move || {
                query_handle.reconciliation_report(summary, lane_summaries_for_query.as_slice())
            })
            .await
            {
                Ok(report) => Ok((report, lane_summaries)),
                Err(response) => return response,
            }
        }
        (Err(error), _) => Err(error),
        (Ok(_), None) => Err(ServiceError::InstanceNotFound(instance_id.clone())),
    };
    let stream_statuses = stream_status_map(state.data_plane.snapshot_streams(unix_now_ms(), 1));
    match report {
        Ok((report, lane_summaries)) => (
            StatusCode::OK,
            Json(json!(reconciliation_report_view(
                report,
                lane_summaries.as_slice(),
                &stream_statuses,
            ))),
        )
            .into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn start_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.start_instance(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn stop_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.stop_instance(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn pause_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.pause_instance(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn resume_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.resume_instance(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn reconcile_bot_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.reconcile_instance(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn cancel_bot_open_orders_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.cancel_open_orders(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn close_bot_positions_handler(
    State(state): State<HttpState>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.close_positions(&instance_id) {
        Ok(summary) => (StatusCode::OK, Json(json!(summary))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn enable_kill_switch_handler(
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.set_kill_switch(true) {
        Ok(()) => (StatusCode::OK, Json(json!(runtime.status()))).into_response(),
        Err(error) => service_error_into_response(&error),
    }
}

pub(crate) async fn disable_kill_switch_handler(
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let mut runtime = state.runtime.write().await;
    match runtime.set_kill_switch(false) {
        Ok(()) => (StatusCode::OK, Json(json!(runtime.status()))).into_response(),
        Err(error) => service_error_into_response(&error),
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
        ServiceError::Storage(_) | ServiceError::Json(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };

    if status.is_server_error() {
        error!(status = %status, error = %error, "request failed with server error");
    } else if status == StatusCode::SERVICE_UNAVAILABLE {
        warn!(status = %status, error = %error, "request failed due to unavailable dependency");
    } else {
        info!(status = %status, error = %error, "request rejected");
    }

    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
        .into_response()
}
