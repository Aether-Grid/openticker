use anyhow::{Context, Result, bail};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use reqwest::{Client, Method};
use serde_json::Value;
use std::time::Duration;
use tracing::debug;

/// Characters that must be percent-encoded inside a single URL path segment.
///
/// We start from the controls and add every character that is structurally
/// significant in a URL so an instance id like `a/b`, `a?b`, or `a#b` cannot
/// break out of its path segment and retarget the request.
const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'/')
    .add(b'?')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'=')
    .add(b'+')
    .add(b';')
    .add(b'@')
    .add(b':')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'|')
    .add(b'\\')
    .add(b'^');

/// Connect timeout for operator API calls.
const API_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Overall per-request timeout for operator API calls.
const API_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Percent-encodes a single URL path segment (e.g. an instance id).
///
/// IDs are interpolated into request paths via `format!`; without encoding, an
/// id containing `/`, `?`, or `#` would alter the path/query/fragment and could
/// route a trading command to the wrong endpoint.
pub(crate) fn encode_path_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT).to_string()
}

/// Builds the shared reqwest client used for all operator API calls.
///
/// Uses short, interactive-friendly timeouts so a hung server cannot stall the
/// CLI (or the ~1s dashboard refresh loop) on reqwest's multi-second defaults.
pub(crate) fn build_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(API_CONNECT_TIMEOUT)
        .timeout(API_REQUEST_TIMEOUT)
        .build()
        .context("failed to build HTTP client")
}

pub(crate) async fn fetch_and_print(api_url: &str, path: &str) -> Result<()> {
    let payload = api_get_json(api_url, path).await?;
    print_api_payload(payload)
}

pub(crate) async fn post_and_print(api_url: &str, path: &str) -> Result<()> {
    let payload = api_post_json(api_url, path).await?;
    print_journaling_only_warning(path);
    print_api_payload(payload)
}

pub(crate) async fn api_post_json(api_url: &str, path: &str) -> Result<Value> {
    api_request_json(api_url, path, Method::POST).await
}

/// POSTs to the API reusing a caller-supplied client. Intended for loops that
/// issue many requests so they share one connection pool across iterations.
pub(crate) async fn api_post_json_with_client(
    client: &Client,
    api_url: &str,
    path: &str,
) -> Result<Value> {
    api_request_json_with_client(client, api_url, path, Method::POST).await
}

pub(crate) fn print_json(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

async fn api_get_json(api_url: &str, path: &str) -> Result<Value> {
    api_request_json(api_url, path, Method::GET).await
}

fn print_journaling_only_warning(path: &str) {
    if path.ends_with("/cancel-open-orders") {
        eprintln!(
            "!!! WARNING: this endpoint is currently journaling-only and does not submit an authoritative broker close/cancel !!!"
        );
    }
}

async fn api_request_json(api_url: &str, path: &str, method: Method) -> Result<Value> {
    // One-shot CLI subcommands build a client locally; loops that issue many
    // requests should instead build a client once and call
    // `api_request_json_with_client` to reuse the connection pool.
    let client = build_client()?;
    api_request_json_with_client(&client, api_url, path, method).await
}

/// Issues a JSON API request using a caller-supplied client, allowing callers
/// that make repeated requests (e.g. the auto-tick loop) to reuse a single
/// connection pool instead of rebuilding a `Client` per call.
async fn api_request_json_with_client(
    client: &Client,
    api_url: &str,
    path: &str,
    method: Method,
) -> Result<Value> {
    let url = format!("{}{}", api_url.trim_end_matches('/'), path);
    debug!(method = %method, %url, "sending API request");
    let response = client
        .request(method.clone(), &url)
        .send()
        .await
        .with_context(|| format!("{method} {url} failed to send request"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("{method} {url} ({status}) failed to read response body"))?;
    debug!(method = %method, %url, %status, "received API response");

    if !status.is_success() {
        bail!("{method} {url} failed ({status}): {body}");
    }

    serde_json::from_str::<Value>(&body)
        .with_context(|| format!("{method} {url} ({status}) response was not valid JSON"))
}

fn print_api_payload(payload: Value) -> Result<()> {
    if let Some(mode_banner) = extract_live_mode_banner(&payload) {
        eprintln!("!!! {mode_banner} !!!");
    } else if !payload_has_recognized_mode_field(&payload) {
        // The live-mode banner is a safety feature: if a schema change renames
        // every field we probe, detection silently disables. Log it so the drift
        // is visible rather than failing open without a trace.
        debug!(
            "no recognized live-mode field found in API payload; live-mode banner detection may be stale"
        );
    }
    print_json(payload)
}

/// Reports whether the payload contains at least one of the field names that
/// [`extract_live_mode_banner`] recognizes, searched recursively.
///
/// Used purely for logging: a payload with none of these fields anywhere is a
/// signal that the response schema may have drifted away from what the banner
/// detector expects.
fn payload_has_recognized_mode_field(payload: &Value) -> bool {
    match payload {
        Value::Object(object) => {
            const MODE_FIELDS: [&str; 4] =
                ["live_mode_active", "mode_banner", "execution_mode", "mode"];
            if MODE_FIELDS.iter().any(|field| object.contains_key(*field)) {
                return true;
            }
            object.values().any(payload_has_recognized_mode_field)
        }
        Value::Array(items) => items.iter().any(payload_has_recognized_mode_field),
        _ => false,
    }
}

fn extract_live_mode_banner(payload: &Value) -> Option<String> {
    match payload {
        Value::Object(object) => {
            if object
                .get("live_mode_active")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Some(
                    object
                        .get("mode_banner")
                        .and_then(Value::as_str)
                        .map_or_else(
                            || "LIVE MODE ACTIVE - real capital may be at risk".to_owned(),
                            ToOwned::to_owned,
                        ),
                );
            }

            if object
                .get("execution_mode")
                .and_then(Value::as_str)
                .is_some_and(|mode| mode.eq_ignore_ascii_case("live"))
                || object
                    .get("mode")
                    .and_then(Value::as_str)
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("live"))
            {
                return Some("LIVE MODE ACTIVE - real capital may be at risk".to_owned());
            }

            for key in ["items", "connector_statuses", "instances"] {
                if let Some(value) = object.get(key)
                    && let Some(mode_banner) = extract_live_mode_banner(value)
                {
                    return Some(mode_banner);
                }
            }

            None
        }
        Value::Array(items) => items.iter().find_map(extract_live_mode_banner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_path_segment_escapes_path_breaking_chars() {
        assert_eq!(encode_path_segment("aapl"), "aapl");
        assert_eq!(encode_path_segment("a/b"), "a%2Fb");
        assert_eq!(encode_path_segment("a?b"), "a%3Fb");
        assert_eq!(encode_path_segment("a#b"), "a%23b");
        assert_eq!(encode_path_segment("../secrets"), "..%2Fsecrets");
    }

    #[test]
    fn encode_path_segment_keeps_segment_within_intended_path() {
        // An id that tries to escape the segment must stay a single segment.
        let id = "evil/../admin?drop=1";
        let path = format!("/v1/bots/{}/close-positions", encode_path_segment(id));
        assert!(
            !path.contains("/v1/bots/evil/"),
            "id escaped its segment: {path}"
        );
        assert!(!path.contains('?'), "id introduced a query string: {path}");
        assert_eq!(
            path,
            "/v1/bots/evil%2F..%2Fadmin%3Fdrop%3D1/close-positions"
        );
    }
}
