use crate::router::build_router;
use crate::state::HttpState;
use anyhow::Result;
use openticker_config::{load_from_dir, load_sources_from_dir};
use openticker_runtime::{Runtime, RuntimePollingSupervisor};
use std::path::Path as FsPath;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};

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
/// # Errors
///
/// Returns an error when binding the socket fails or the server exits with an error.
pub async fn serve(bind_addr: &str, state: HttpState) -> Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!(bind_addr, "starting HTTP API server");
    let polling_supervisor =
        RuntimePollingSupervisor::start(&state.runtime, Arc::clone(&state.data_plane));

    let serve_result = axum::serve(listener, build_router(state)).await;
    polling_supervisor.shutdown().await;
    if let Err(error) = &serve_result {
        error!(error = %error, "HTTP API server terminated with error");
    }
    serve_result?;
    info!(bind_addr, "HTTP API server shut down");
    Ok(())
}
