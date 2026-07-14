use crate::state::HttpState;
use axum::Json;
use axum::extract::State;
use openticker_connectors::connector_matrix;
use serde_json::json;

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
