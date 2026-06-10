//! Config write endpoints: validate-then-write TOML plus hot apply.
//!
//! Every endpoint follows the same pipeline, entirely under
//! `state.config_write_lock`:
//!
//! 1. reject when the service has no managed config directory (501);
//! 2. enforce the optional `If-Match: <generation>` stale guard (409/400);
//! 3. load the on-disk sources (disk is the truth, not the in-memory bundle);
//! 4. mutate exactly one entity in memory and render its file content;
//! 5. validate the full candidate bundle (422, nothing written);
//! 6. run change-set validation against the running instances (409, nothing
//!    written);
//! 7. prebuild the runtime from the candidate (422, nothing written);
//! 8. persist atomically (500 on I/O failure);
//! 9. swap the runtime via [`apply_bundle_and_record`].

use crate::config_ops::{
    ConfigApplyError, ReloadTrigger, ReloadViolation, apply_bundle_and_record,
    validate_reload_change_set, violation_summary,
};
use crate::state::{ErrorResponse, HttpState};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use openticker_config::{
    AccountConfigUpdate, ConfigBundle, ConfigSources, GlobalConfig, InstanceConfig,
    RiskProfileConfig, SourceFile, load_sources_from_dir, render_new_document,
    render_updated_document, write_atomic,
};
use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::MutexGuard;
use tracing::warn;

/// The persistence step computed by each endpoint's mutation.
enum WriteOp {
    /// Atomically replace (or create) `path` with `contents`.
    Write { path: PathBuf, contents: String },
    /// Remove `path` from disk.
    Delete { path: PathBuf },
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: message.into(),
        }),
    )
        .into_response()
}

fn violations_response(message: &str, violations: &[ReloadViolation]) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({
            "error": message,
            "violations": violations,
        })),
    )
        .into_response()
}

/// Validates the optional `If-Match: <generation>` header against the current
/// reload generation, returning the rejection response when the guard fails.
/// Must be called while holding the config write lock so the generation
/// cannot move between the check and the apply.
fn check_if_match(state: &HttpState, headers: &HeaderMap) -> Option<Response> {
    let value = headers.get(axum::http::header::IF_MATCH)?;
    let Some(expected) = value
        .to_str()
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
    else {
        return Some(error_response(
            StatusCode::BAD_REQUEST,
            "invalid If-Match header: expected a config generation number",
        ));
    };
    let current = state
        .reload_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    if expected != current {
        let violations = vec![ReloadViolation {
            code: "stale_generation",
            scope: "global".to_owned(),
            message: format!(
                "config write rejected: If-Match generation {expected} does not match current generation {current}"
            ),
        }];
        return Some(violations_response(
            &format!("config write failed: {}", violation_summary(&violations)),
            &violations,
        ));
    }
    None
}

/// Shared prologue: resolve the managed config dir, take the write lock,
/// enforce `If-Match`, and load the on-disk sources.
async fn begin_config_write<'a>(
    state: &'a HttpState,
    headers: &HeaderMap,
) -> Result<(MutexGuard<'a, ()>, ConfigSources), Response> {
    let Some(config_dir) = state.config_dir.clone() else {
        return Err(error_response(
            StatusCode::NOT_IMPLEMENTED,
            "service was started without a managed config directory",
        ));
    };

    let guard = state.config_write_lock.lock().await;
    if let Some(rejection) = check_if_match(state, headers) {
        return Err(rejection);
    }

    let sources = load_sources_from_dir(&config_dir).map_err(|error| {
        warn!(error = %error, "config write failed while loading on-disk sources");
        error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("config write failed: {error}"),
        )
    })?;

    Ok((guard, sources))
}

/// Shared epilogue: validate the candidate bundle, run change-set validation,
/// prebuild the runtime, persist the write, and hot-apply. Nothing is written
/// to disk unless every validation step passes.
async fn finish_config_write(
    state: &HttpState,
    guard: &MutexGuard<'_, ()>,
    candidate: ConfigBundle,
    op: WriteOp,
) -> Result<u64, Response> {
    candidate.validate().map_err(|error| {
        error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("config write failed: {error}"),
        )
    })?;

    let running_instance_ids = {
        let runtime = state.runtime.read().await;
        runtime
            .list_instances()
            .into_iter()
            .filter(|instance| {
                matches!(
                    instance.state,
                    openticker_runtime::InstanceRuntimeState::Running
                )
            })
            .map(|instance| instance.id)
            .collect::<HashSet<_>>()
    };

    {
        let current_bundle = state.config_bundle.read().await;
        if let Some(current) = current_bundle.as_ref() {
            let violations = validate_reload_change_set(current, &candidate, &running_instance_ids);
            if !violations.is_empty() {
                let summary = violation_summary(&violations);
                warn!(error = %summary, "config write rejected by change-set validation");
                return Err(violations_response(
                    &format!("config write failed: {summary}"),
                    &violations,
                ));
            }
        }
    }

    let prebuilt =
        openticker_runtime::Runtime::from_config_with_storage(&candidate).map_err(|error| {
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("config write failed: {error}"),
            )
        })?;

    match &op {
        WriteOp::Write { path, contents } => write_atomic(path, contents).map_err(|error| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("config write failed: {error}"),
            )
        })?,
        WriteOp::Delete { path } => std::fs::remove_file(path).map_err(|error| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("config write failed: {error}"),
            )
        })?,
    }

    apply_bundle_and_record(
        state,
        candidate,
        Some(prebuilt),
        ReloadTrigger::ConfigWrite,
        guard,
    )
    .await
    .map_err(|error| match error {
        ConfigApplyError::RuntimeBuild(message) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("config write failed: {message}"),
        ),
        ConfigApplyError::NotManaged => error_response(
            StatusCode::NOT_IMPLEMENTED,
            "service was started without a managed config directory",
        ),
        ConfigApplyError::Load(error) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("config write failed: {error}"),
        ),
        ConfigApplyError::ChangeSet(violations) => violations_response(
            &format!("config write failed: {}", violation_summary(&violations)),
            &violations,
        ),
    })
}

async fn write_success_response(
    state: &HttpState,
    http_status: StatusCode,
    status_word: &'static str,
    generation: u64,
    entity: Option<(&'static str, serde_json::Value)>,
) -> Response {
    let service = {
        let runtime = state.runtime.read().await;
        runtime.status()
    };
    let mut body = json!({
        "status": status_word,
        "generation": generation,
        "service": service,
    });
    if let (Some((key, value)), Some(map)) = (entity, body.as_object_mut()) {
        map.insert(key.to_owned(), value);
    }
    (http_status, Json(body)).into_response()
}

fn render_update_error(error: &openticker_config::ConfigError) -> Response {
    error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("config write failed: {error}"),
    )
}

/// `PUT /v1/config/global`
pub(crate) async fn put_config_global_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<GlobalConfig>,
) -> Response {
    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let contents = match render_updated_document(&sources.global.raw, &body) {
        Ok(contents) => contents,
        Err(error) => return render_update_error(&error),
    };
    let path = sources.global.path.clone();
    sources.global.value = body;
    let entity = json!(sources.global.value);
    let candidate = sources.to_bundle();

    match finish_config_write(&state, &guard, candidate, WriteOp::Write { path, contents }).await {
        Ok(generation) => {
            write_success_response(
                &state,
                StatusCode::OK,
                "updated",
                generation,
                Some(("global", entity)),
            )
            .await
        }
        Err(response) => response,
    }
}

/// `PUT /v1/config/bots/{id}`
pub(crate) async fn put_config_bot_handler(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<InstanceConfig>,
) -> Response {
    if body.id != bot_id {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "config write failed: body id `{}` does not match path id `{bot_id}`",
                body.id
            ),
        );
    }

    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let Some(index) = sources
        .instances
        .iter()
        .position(|file| file.value.id == bot_id)
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            format!("bot `{bot_id}` does not exist in the managed config"),
        );
    };

    let contents = match render_updated_document(&sources.instances[index].raw, &body) {
        Ok(contents) => contents,
        Err(error) => return render_update_error(&error),
    };
    let path = sources.instances[index].path.clone();
    sources.instances[index].value = body;
    let entity = json!(sources.instances[index].value);
    let candidate = sources.to_bundle();

    match finish_config_write(&state, &guard, candidate, WriteOp::Write { path, contents }).await {
        Ok(generation) => {
            write_success_response(
                &state,
                StatusCode::OK,
                "updated",
                generation,
                Some(("bot", entity)),
            )
            .await
        }
        Err(response) => response,
    }
}

/// Allows only filename-safe bot ids so a crafted id can never escape the
/// bots directory (path traversal guard).
fn bot_id_is_safe_filename(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `POST /v1/config/bots`
pub(crate) async fn create_config_bot_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<InstanceConfig>,
) -> Response {
    if !bot_id_is_safe_filename(&body.id) {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "config write failed: bot id `{}` is not a safe filename (allowed: alphanumeric, `-`, `_`)",
                body.id
            ),
        );
    }

    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let path = sources.bots_dir.join(format!("{}.toml", body.id));
    if sources.instance_by_id(&body.id).is_some() || path.exists() {
        return error_response(
            StatusCode::CONFLICT,
            format!("bot `{}` already exists in the managed config", body.id),
        );
    }

    let contents = match render_new_document(&body) {
        Ok(contents) => contents,
        Err(error) => return render_update_error(&error),
    };
    let entity = json!(body);
    sources.instances.push(SourceFile {
        path: path.clone(),
        raw: contents.clone(),
        value: body,
    });
    let candidate = sources.to_bundle();

    match finish_config_write(&state, &guard, candidate, WriteOp::Write { path, contents }).await {
        Ok(generation) => {
            write_success_response(
                &state,
                StatusCode::CREATED,
                "created",
                generation,
                Some(("bot", entity)),
            )
            .await
        }
        Err(response) => response,
    }
}

/// `DELETE /v1/config/bots/{id}`
pub(crate) async fn delete_config_bot_handler(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let Some(index) = sources
        .instances
        .iter()
        .position(|file| file.value.id == bot_id)
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            format!("bot `{bot_id}` does not exist in the managed config"),
        );
    };

    let removed = sources.instances.remove(index);
    let candidate = sources.to_bundle();

    match finish_config_write(
        &state,
        &guard,
        candidate,
        WriteOp::Delete { path: removed.path },
    )
    .await
    {
        Ok(generation) => {
            write_success_response(&state, StatusCode::OK, "deleted", generation, None).await
        }
        Err(response) => response,
    }
}

/// `PUT /v1/config/risk-profiles/{id}`
pub(crate) async fn put_config_risk_profile_handler(
    State(state): State<HttpState>,
    Path(profile_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RiskProfileConfig>,
) -> Response {
    if body.id != profile_id {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "config write failed: body id `{}` does not match path id `{profile_id}`",
                body.id
            ),
        );
    }

    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let Some(index) = sources
        .risk_profiles
        .iter()
        .position(|file| file.value.id == profile_id)
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            format!("risk profile `{profile_id}` does not exist in the managed config"),
        );
    };

    let contents = match render_updated_document(&sources.risk_profiles[index].raw, &body) {
        Ok(contents) => contents,
        Err(error) => return render_update_error(&error),
    };
    let path = sources.risk_profiles[index].path.clone();
    sources.risk_profiles[index].value = body;
    let entity = json!(sources.risk_profiles[index].value);
    let candidate = sources.to_bundle();

    match finish_config_write(&state, &guard, candidate, WriteOp::Write { path, contents }).await {
        Ok(generation) => {
            write_success_response(
                &state,
                StatusCode::OK,
                "updated",
                generation,
                Some(("risk_profile", entity)),
            )
            .await
        }
        Err(response) => response,
    }
}

/// `PUT /v1/config/accounts/{id}`
///
/// The body is an [`AccountConfigUpdate`], which structurally cannot carry
/// `id` or the secret env references; those are copied from the on-disk
/// account. The response uses the redacted effective-account shape, so env
/// var names never appear in any response.
pub(crate) async fn put_config_account_handler(
    State(state): State<HttpState>,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<AccountConfigUpdate>,
) -> Response {
    let (guard, mut sources) = match begin_config_write(&state, &headers).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };

    let Some(index) = sources
        .accounts
        .iter()
        .position(|file| file.value.id == account_id)
    else {
        return error_response(
            StatusCode::NOT_FOUND,
            format!("account `{account_id}` does not exist in the managed config"),
        );
    };

    let next_account = body.into_account(&sources.accounts[index].value);
    let contents = match render_updated_document(&sources.accounts[index].raw, &next_account) {
        Ok(contents) => contents,
        Err(error) => return render_update_error(&error),
    };
    let path = sources.accounts[index].path.clone();
    sources.accounts[index].value = next_account;
    let candidate = sources.to_bundle();
    let entity = candidate
        .effective_config()
        .accounts
        .into_iter()
        .find(|account| account.id == account_id)
        .map_or(serde_json::Value::Null, |account| json!(account));

    match finish_config_write(&state, &guard, candidate, WriteOp::Write { path, contents }).await {
        Ok(generation) => {
            write_success_response(
                &state,
                StatusCode::OK,
                "updated",
                generation,
                Some(("account", entity)),
            )
            .await
        }
        Err(response) => response,
    }
}
