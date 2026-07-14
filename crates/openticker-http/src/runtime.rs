use crate::config_watcher::ConfigWatcher;
use crate::router::{API_TOKEN_ENV, build_router_with_token};
use crate::state::HttpState;
use anyhow::Result;
use openticker_config::{load_from_dir, load_sources_from_dir};
use openticker_runtime::{Runtime, RuntimePollingSupervisor};
use std::path::Path as FsPath;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

/// Loads config from `config_dir` and constructs HTTP state with persisted runtime storage.
///
/// # Errors
///
/// Returns an error when configuration loading or runtime initialization fails.
pub fn load_http_state(config_dir: impl AsRef<FsPath>) -> Result<HttpState> {
    let config_dir = config_dir.as_ref().to_path_buf();
    // load_from_dir also loads dotenv before parsing, so it stays the source of
    // truth for the bundle; the sources are only consulted for resolved paths.
    let bundle = load_from_dir(&config_dir)?;
    let sources = load_sources_from_dir(&config_dir)?;
    let runtime = Runtime::from_config_with_storage(&bundle)?;
    let mut state = HttpState::with_config(runtime, config_dir, bundle);
    state.bots_dir = Some(sources.bots_dir);
    Ok(state)
}

/// Serves the HTTP API on `bind_addr` until the server exits.
///
/// Bearer-token authentication is opt-in: when the `OPENTICKER_API_TOKEN`
/// environment variable ([`API_TOKEN_ENV`]) is set and non-empty, every API
/// endpoint requires `Authorization: Bearer <token>`; health/readiness/
/// metrics probes and the embedded dashboard assets remain open. When unset
/// or empty, the API is unauthenticated and a warning is logged at startup.
///
/// # Errors
///
/// Returns an error when binding the socket fails or the server exits with an error.
pub async fn serve(bind_addr: &str, state: HttpState) -> Result<()> {
    // Composition root for API authentication: `load_http_state` has already
    // run `load_from_dir`, which loads dotenv, so the token may come from the
    // process environment or a `.env` file.
    let api_token = std::env::var(API_TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty());
    if api_token.is_some() {
        info!("HTTP API bearer-token authentication enabled via {API_TOKEN_ENV}");
    } else {
        warn!(
            "{API_TOKEN_ENV} is not set; the HTTP API is served WITHOUT authentication — \
             only expose this server on localhost or behind an authenticating proxy"
        );
    }

    let listener = TcpListener::bind(bind_addr).await?;
    info!(bind_addr, "starting HTTP API server");
    let polling_supervisor =
        RuntimePollingSupervisor::start(&state.runtime, Arc::clone(&state.data_plane));
    let config_watcher = state.config_dir.clone().and_then(|config_dir| {
        ConfigWatcher::start(state.clone(), config_dir, state.bots_dir.clone())
    });

    let serve_result = axum::serve(listener, build_router_with_token(state, api_token)).await;
    if let Some(config_watcher) = config_watcher {
        config_watcher.shutdown().await;
    }
    polling_supervisor.shutdown().await;
    if let Err(error) = &serve_result {
        error!(error = %error, "HTTP API server terminated with error");
    }
    serve_result?;
    info!(bind_addr, "HTTP API server shut down");
    Ok(())
}
