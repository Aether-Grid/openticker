use super::service_error_into_response;
use crate::state::HttpState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

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
