use super::views::{
    bot_detail_view, bot_summary_view, reconciliation_report_view, stream_status_map,
};
use super::{clamped_limit, run_blocking_query, service_error_into_response, unix_now_ms};
use crate::state::{ErrorResponse, HttpState};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use openticker_runtime::ServiceError;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

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

#[derive(Debug, Deserialize)]
pub(crate) struct BotCyclesQuery {
    limit: Option<usize>,
    symbol: Option<String>,
    phase: Option<String>,
    outcome: Option<String>,
    bar_timestamp: Option<String>,
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
    let limit = clamped_limit(query.limit, 50);
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
