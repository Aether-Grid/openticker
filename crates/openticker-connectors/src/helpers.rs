use crate::error::ConnectorError;
use crate::types::ConnectorKind;
use openticker_execution::ExecutionRequest;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) fn default_blocking_http_client(
    kind: ConnectorKind,
) -> Result<reqwest::blocking::Client, ConnectorError> {
    // Keep one shared blocking client alive for the process lifetime so reqwest's
    // internal runtime is never dropped from an async Tokio worker thread.
    // Build the client on a dedicated OS thread, because constructing a
    // reqwest::blocking::Client inside an async runtime context can panic.
    static SHARED_HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

    if let Some(client) = SHARED_HTTP_CLIENT.get() {
        return Ok(client.clone());
    }

    let builder = std::thread::Builder::new().name("openticker-http-client-init".to_owned());
    let handle = builder
        .spawn(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
        })
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind,
            detail: format!("failed to spawn HTTP client init thread: {error}"),
        })?;

    let client = handle
        .join()
        .map_err(|_| ConnectorError::RemoteSnapshot {
            kind,
            detail: "HTTP client init thread panicked".to_owned(),
        })?
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind,
            detail: format!("failed to construct HTTP client: {error}"),
        })?;

    let _ = SHARED_HTTP_CLIENT.set(client);

    SHARED_HTTP_CLIENT
        .get()
        .cloned()
        .ok_or_else(|| ConnectorError::RemoteSnapshot {
            kind,
            detail: "shared HTTP client was not initialized".to_owned(),
        })
}

pub(crate) fn run_in_blocking_thread<T, F>(
    kind: ConnectorKind,
    operation: &'static str,
    task: F,
) -> Result<T, ConnectorError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ConnectorError> + Send + 'static,
{
    let thread_name = format!("openticker-{operation}");
    let handle = std::thread::Builder::new()
        .name(thread_name)
        .spawn(task)
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind,
            detail: format!("failed to spawn `{operation}` thread: {error}"),
        })?;

    handle.join().map_err(|_| ConnectorError::RemoteSnapshot {
        kind,
        detail: format!("`{operation}` thread panicked"),
    })?
}

pub(crate) fn resolve_secret_env_value(
    kind: ConnectorKind,
    field: &'static str,
    env_var: Option<&str>,
) -> Result<String, ConnectorError> {
    let env_var = env_var.ok_or(ConnectorError::MissingCredentialReference { kind, field })?;
    std::env::var(env_var).map_err(|_| ConnectorError::MissingCredentialValue {
        kind,
        env_var: env_var.to_owned(),
    })
}

#[must_use]
pub(crate) fn deterministic_remote_client_order_id(
    kind: ConnectorKind,
    request: &ExecutionRequest,
) -> String {
    let seed = format!(
        "{}:{}:{}:{}:{:?}:{:.10}",
        kind.as_str(),
        request.instance_id,
        request.symbol,
        request.timestamp.timestamp_millis(),
        request.intent,
        request.quantity,
    );
    let digest = Sha256::digest(seed.as_bytes());
    let digest_hex = hex::encode(digest);
    format!("{}-{}", kind.as_str(), &digest_hex[..24])
}

#[must_use]
pub(crate) fn format_decimal_quantity(value: f64) -> String {
    let mut formatted = format!("{value:.12}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

#[must_use]
pub(crate) fn unix_now_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}
