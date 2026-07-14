use crate::state::HttpState;
use axum::Json;
use axum::extract::State;
use serde_json::json;

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
