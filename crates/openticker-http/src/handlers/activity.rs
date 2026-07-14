use super::{LimitQuery, clamped_limit, run_blocking_query, service_error_into_response};
use crate::state::HttpState;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub(crate) struct EventsQuery {
    limit: Option<usize>,
    scope: Option<String>,
    entity_id: Option<String>,
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

pub(crate) async fn list_events_handler(
    State(state): State<HttpState>,
    Query(query): Query<EventsQuery>,
) -> impl IntoResponse {
    let query_handle = state.query.read().await.clone();
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
    let limit = clamped_limit(query.limit, 100);
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
