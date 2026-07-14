use super::views::{
    bot_detail_view, bot_summary_view, reconciliation_report_view, stream_status_map,
};
use super::{
    LimitQuery, clamped_limit, run_blocking_query, service_error_into_response, unix_now_ms,
};
use crate::constants::{
    BOT_SNAPSHOT_ORDERS_LIMIT, BOT_SNAPSHOT_POSITIONS_LIMIT, BOT_SNAPSHOT_TIMELINE_LIMIT,
    DASHBOARD_SNAPSHOT_DEFAULT_LIMIT, STREAM_SPARKLINE_LIMIT,
};
use crate::state::{
    DashboardBotSnapshot, DashboardHomeSnapshot, DashboardRiskDecisionsSnapshot, HttpState,
};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use openticker_connectors::connector_matrix;
use openticker_runtime::ServiceError;
use serde_json::json;
use std::collections::HashMap;

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

pub(crate) async fn dashboard_snapshot_handler(
    State(state): State<HttpState>,
    Query(query): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = clamped_limit(query.limit, DASHBOARD_SNAPSHOT_DEFAULT_LIMIT);
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
