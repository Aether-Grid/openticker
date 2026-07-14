use anyhow::{Context, Result, bail};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::models::{
    DashboardBotSummary, DashboardConnectorStatus, DashboardOrderRecord,
    DashboardRiskDecisionResponse, DashboardRuntimeEvent, DashboardServiceStatus,
    DashboardSnapshot,
};

pub(super) async fn fetch_snapshot(
    client: &Client,
    api_url: &str,
    limit: usize,
) -> Result<DashboardSnapshot> {
    let service =
        api_get_typed::<DashboardServiceStatus>(client, api_url, "/v1/service/status").await?;
    let bots = api_get_typed::<Vec<DashboardBotSummary>>(client, api_url, "/v1/bots").await?;
    let connectors =
        api_get_typed::<Vec<DashboardConnectorStatus>>(client, api_url, "/v1/connectors/status")
            .await?;
    let risk_path = format!("/v1/risk-decisions?limit={limit}");
    let risk_response =
        api_get_typed::<DashboardRiskDecisionResponse>(client, api_url, &risk_path).await?;
    let orders_path = format!("/v1/orders?limit={limit}");
    let orders = api_get_typed::<Vec<DashboardOrderRecord>>(client, api_url, &orders_path).await?;
    let events_path = format!("/v1/events?limit={limit}");
    let events = api_get_typed::<Vec<DashboardRuntimeEvent>>(client, api_url, &events_path).await?;

    Ok(DashboardSnapshot {
        service,
        bots,
        connectors,
        risk_count: risk_response.count,
        risk_decisions: risk_response.items,
        orders,
        events,
    })
}

async fn api_get_typed<T: DeserializeOwned>(
    client: &Client,
    api_url: &str,
    path: &str,
) -> Result<T> {
    let payload = api_request_json(client, api_url, path, Method::GET).await?;
    serde_json::from_value(payload).with_context(|| {
        format!(
            "response from `{path}` did not match expected shape `{}`",
            std::any::type_name::<T>()
        )
    })
}

pub(super) async fn api_request_json(
    client: &Client,
    api_url: &str,
    path: &str,
    method: Method,
) -> Result<Value> {
    let url = format!("{}{}", api_url.trim_end_matches('/'), path);
    let response = client
        .request(method, &url)
        .send()
        .await
        .with_context(|| format!("failed to send request to {url}"))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("failed to read response body from {url}"))?;

    if !status.is_success() {
        bail!("request to {url} failed ({status}): {body}");
    }

    serde_json::from_str::<Value>(&body)
        .with_context(|| format!("response from {url} was not valid JSON"))
}
