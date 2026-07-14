use super::{LimitQuery, clamped_limit, service_error_into_response, unix_now_ms};
use crate::constants::{DEFAULT_STREAM_BARS_LIMIT, STREAM_SPARKLINE_LIMIT};
use crate::state::{ErrorResponse, HttpState};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use openticker_core::Timeframe;
use openticker_dataplane::{StreamKey, StreamStatus};
use serde_json::json;
use std::str::FromStr;

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
    let limit = clamped_limit(query.limit, DEFAULT_STREAM_BARS_LIMIT);

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
    let limit = clamped_limit(query.limit, DEFAULT_STREAM_BARS_LIMIT).max(1);

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
