use anyhow::{Context, Result, bail};
use reqwest::{Client, Method};
use serde_json::Value;
use tracing::debug;

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
    let url = format!("{}{}", api_url.trim_end_matches('/'), path);
    let client = Client::new();
    debug!(method = %method, %url, "sending API request");
    let response = client
        .request(method.clone(), &url)
        .send()
        .await
        .with_context(|| format!("failed to send request to {url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("failed to read response body from {url}"))?;
    debug!(method = %method, %url, %status, "received API response");

    if !status.is_success() {
        bail!("request to {url} failed ({status}): {body}");
    }

    serde_json::from_str::<Value>(&body)
        .with_context(|| format!("response from {url} was not valid JSON"))
}

fn print_api_payload(payload: Value) -> Result<()> {
    if let Some(mode_banner) = extract_live_mode_banner(&payload) {
        eprintln!("!!! {mode_banner} !!!");
    }
    print_json(payload)
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
