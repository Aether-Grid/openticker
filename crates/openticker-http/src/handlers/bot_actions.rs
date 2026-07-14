use super::{service_error_into_response, unix_now_ms};
use crate::state::{ErrorResponse, HttpState};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use openticker_core::{IndicatorSignal, OhlcvBar, SignalPhase};
use openticker_data::NormalizedTrade;
use serde::Deserialize;
use serde_json::json;
use tracing::error;

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
                    error!(error = %error, "failed to record fetched bar to data plane");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "internal error".to_owned(),
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
