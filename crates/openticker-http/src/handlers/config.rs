use crate::config_ops::{ConfigApplyError, ReloadTrigger, reload_from_disk, violation_summary};
use crate::state::{ErrorResponse, HttpState};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde_json::json;

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

pub(crate) async fn config_reload_handler(State(state): State<HttpState>) -> impl IntoResponse {
    match reload_from_disk(&state, ReloadTrigger::ManualApi).await {
        Ok(reloaded) => {
            let runtime = state.runtime.read().await;
            // Both success shapes carry the current config generation so a
            // client can bootstrap its `If-Match` token from this one call.
            let generation = state
                .reload_generation
                .load(std::sync::atomic::Ordering::SeqCst);
            (
                StatusCode::OK,
                Json(json!({
                    "status": if reloaded { "reloaded" } else { "no_change" },
                    "reloaded": reloaded,
                    "generation": generation,
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
    let history = state.reload_status.read().await;
    // Read the generation while holding the history lock so the response can
    // never pair a bumped generation with a history that lacks its entry.
    let generation = state
        .reload_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let newest_first = history.iter().rev().cloned().collect::<Vec<_>>();
    Json(json!({
        "generation": generation,
        "last": newest_first.first(),
        "history": newest_first,
    }))
}
