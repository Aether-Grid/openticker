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

/// Maximum number of characters retained from a symbol when it is embedded in
/// an error message. Exchange symbols are short (typically <=20 chars); this
/// bound keeps oversized or hostile inputs from polluting logs.
pub(crate) const MAX_ERROR_SYMBOL_LEN: usize = 32;

/// Sanitizes a symbol for safe inclusion in error text and logs.
///
/// Control characters (including newlines, which could be used to forge log
/// lines) are replaced with `?`, and the result is truncated to
/// [`MAX_ERROR_SYMBOL_LEN`] characters with an ellipsis appended when the input
/// was longer. The original symbol is never mutated; this only affects the
/// string used in diagnostics.
#[must_use]
pub(crate) fn sanitize_symbol_for_error(symbol: &str) -> String {
    let mut sanitized: String = symbol
        .chars()
        .take(MAX_ERROR_SYMBOL_LEN)
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect();
    if symbol.chars().nth(MAX_ERROR_SYMBOL_LEN).is_some() {
        sanitized.push('…');
    }
    sanitized
}

/// Classifies an HTTP failure as a rate-limit error when the status code is
/// 429 (too many requests) or 418 (provider IP ban, e.g. Binance `-1003`).
///
/// The retry window is derived from the provider's "banned until <unix-ms>"
/// message when present, falling back to a `Retry-After` header in seconds.
pub(crate) fn rate_limit_error(
    kind: ConnectorKind,
    status: reqwest::StatusCode,
    retry_after_header: Option<&str>,
    detail: String,
) -> Option<ConnectorError> {
    if !matches!(status.as_u16(), 418 | 429) {
        return None;
    }

    let retry_after_ms = parse_banned_until_ms(&detail)
        .and_then(|until_ms| u64::try_from(until_ms.saturating_sub(unix_now_ms())).ok())
        .filter(|window_ms| *window_ms > 0)
        .or_else(|| {
            retry_after_header
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(|seconds| seconds.saturating_mul(1_000))
        });

    Some(ConnectorError::RateLimited {
        kind,
        retry_after_ms,
        detail,
    })
}

/// Returns the `Retry-After` header value of a blocking HTTP response, if any.
pub(crate) fn retry_after_header(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn parse_banned_until_ms(detail: &str) -> Option<i64> {
    let marker = "banned until ";
    let start = detail.find(marker)? + marker.len();
    let digits = detail[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{MAX_ERROR_SYMBOL_LEN, sanitize_symbol_for_error};

    /// A short, clean ASCII symbol must pass through completely unchanged.
    #[test]
    fn short_clean_symbol_passes_through_unchanged() {
        let result = sanitize_symbol_for_error("BTCUSDT");
        assert_eq!(result, "BTCUSDT");
    }

    /// A symbol whose length equals the limit exactly must not gain an ellipsis.
    #[test]
    fn symbol_at_exact_limit_no_ellipsis() {
        let input = "A".repeat(MAX_ERROR_SYMBOL_LEN); // 32 chars
        let result = sanitize_symbol_for_error(&input);
        assert_eq!(result.chars().count(), MAX_ERROR_SYMBOL_LEN);
        assert!(
            !result.ends_with('…'),
            "ellipsis must not appear when input fits exactly"
        );
        assert_eq!(result, input);
    }

    /// A multi-byte UTF-8 string (33 × 'é', each 2 bytes) must be truncated by
    /// CHARACTER count (32 chars kept) without panicking on a byte boundary,
    /// and an ellipsis must be appended because the input exceeds the limit.
    #[test]
    fn multibyte_utf8_truncated_at_char_boundary() {
        let input = "é".repeat(MAX_ERROR_SYMBOL_LEN + 1); // 33 × 'é'
        assert_eq!(
            input.len(),
            (MAX_ERROR_SYMBOL_LEN + 1) * 2,
            "sanity: 66 bytes"
        );

        let result = sanitize_symbol_for_error(&input);

        // Must have truncated to exactly MAX_ERROR_SYMBOL_LEN characters …
        // … plus the trailing '…' ellipsis character.
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(
            chars.last().copied(),
            Some('…'),
            "ellipsis must be appended when input exceeds limit"
        );
        // The kept portion is 32 'é' chars.
        let kept: String = chars[..chars.len() - 1].iter().collect();
        assert_eq!(
            kept.chars().count(),
            MAX_ERROR_SYMBOL_LEN,
            "exactly MAX_ERROR_SYMBOL_LEN characters must be kept before the ellipsis"
        );
        assert!(
            kept.chars().all(|c| c == 'é'),
            "kept characters must be 'é'"
        );
    }

    /// Control characters (newline, carriage-return, tab) must be replaced with '?'.
    #[test]
    fn control_chars_replaced_with_question_mark() {
        let result = sanitize_symbol_for_error("AB\nCD\rEF\t");
        assert_eq!(result, "AB?CD?EF?");
    }

    /// A long string with embedded control chars is truncated AND has controls replaced.
    #[test]
    fn long_string_with_control_chars_truncated_and_sanitized() {
        // Build a 40-char string where every 5th char is a newline.
        let input: String = (0..40u32)
            .map(|i| if i % 5 == 4 { '\n' } else { 'X' })
            .collect();
        let result = sanitize_symbol_for_error(&input);

        // Must end with ellipsis (input is 40 chars > 32).
        assert!(result.ends_with('…'), "ellipsis expected for 40-char input");

        // The non-ellipsis portion must be 32 chars long.
        let body: String = result.chars().take(MAX_ERROR_SYMBOL_LEN).collect();
        assert_eq!(body.chars().count(), MAX_ERROR_SYMBOL_LEN);

        // No raw control characters anywhere in the result.
        assert!(
            !result.chars().any(char::is_control),
            "no control characters may survive sanitization"
        );
    }
}
