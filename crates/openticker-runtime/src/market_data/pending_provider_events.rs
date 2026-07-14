use crate::{OhlcvBar, Runtime, ServiceError, Timeframe};
use openticker_connectors::ConfirmedBarPage;
use openticker_gateway::{Gateway, GatewayError};
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub(crate) struct PendingProviderEvent {
    lane_id: String,
    kind: String,
    payload: Value,
}

#[derive(Debug)]
pub(crate) struct GatewayFetchFailure {
    pub(crate) error: ServiceError,
    pub(crate) provider_events: Vec<PendingProviderEvent>,
}

impl From<ServiceError> for GatewayFetchFailure {
    fn from(error: ServiceError) -> Self {
        Self {
            error,
            provider_events: Vec::new(),
        }
    }
}

impl PendingProviderEvent {
    pub(crate) fn append_to_runtime(&self, runtime: &Runtime) -> Result<(), ServiceError> {
        runtime.append_runtime_event(
            "provider",
            Some(self.lane_id.as_str()),
            self.kind.as_str(),
            self.payload.to_string(),
        )
    }
}

pub(crate) fn append_pending_provider_events(
    runtime: &Runtime,
    events: &[PendingProviderEvent],
) -> Result<(), ServiceError> {
    for event in events {
        event.append_to_runtime(runtime)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn provider_stage_event(
    lane_id: &str,
    kind_prefix: &str,
    account_id: &str,
    connector_kind: &str,
    operation: &str,
    stage: &str,
    summary: impl Into<String>,
    extra: Value,
) -> PendingProviderEvent {
    let mut payload = Map::from_iter([
        ("account_id".to_owned(), serde_json::json!(account_id)),
        (
            "connector_kind".to_owned(),
            serde_json::json!(connector_kind),
        ),
        ("operation".to_owned(), serde_json::json!(operation)),
        ("stage".to_owned(), serde_json::json!(stage)),
        ("summary".to_owned(), serde_json::json!(summary.into())),
    ]);
    if let Value::Object(extra) = extra {
        payload.extend(extra);
    }
    PendingProviderEvent {
        lane_id: lane_id.to_owned(),
        kind: format!("{kind_prefix}.{stage}"),
        payload: Value::Object(payload),
    }
}

fn gateway_account_kind(
    gateway: &Gateway,
    lane_id: &str,
    account_id: &str,
) -> Result<String, ServiceError> {
    gateway
        .account_kind(account_id)
        .map_err(|error| match error {
            GatewayError::UnknownAccount { .. } => ServiceError::InvalidConfiguration(format!(
                "instance `{lane_id}` references unknown account `{account_id}`"
            )),
            other => ServiceError::InvalidConfiguration(other.to_string()),
        })
}

fn validate_gateway_connector_kind(
    gateway: &Gateway,
    lane_id: &str,
    account_id: &str,
    expected_connector_kind: &str,
    context: &str,
) -> Result<(), ServiceError> {
    let account_kind = gateway_account_kind(gateway, lane_id, account_id)?;
    if account_kind != expected_connector_kind {
        return Err(ServiceError::InvalidConfiguration(format!(
            "instance `{lane_id}` {context} `{expected_connector_kind}` does not match account `{account_id}` kind `{account_kind}`"
        )));
    }
    Ok(())
}

pub(crate) fn gateway_fetch_latest_bar_with_events(
    gateway: &Gateway,
    lane_id: &str,
    account_id: &str,
    data_connector: &str,
    symbol: &str,
    timeframe: Timeframe,
) -> Result<(OhlcvBar, Vec<PendingProviderEvent>), GatewayFetchFailure> {
    validate_gateway_connector_kind(
        gateway,
        lane_id,
        account_id,
        data_connector,
        "data_connector",
    )?;
    let request = serde_json::json!({
        "symbol": symbol,
        "timeframe": timeframe,
    });
    let mut events = vec![provider_stage_event(
        lane_id,
        "provider.market_data.latest",
        account_id,
        data_connector,
        "fetch_latest_bar",
        "requested",
        format!("requesting latest {timeframe} bar for {symbol}"),
        serde_json::json!({
            "request": request.clone(),
        }),
    )];

    match gateway.fetch_latest_bar(account_id, symbol, timeframe) {
        Ok(latest_bar) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.latest",
                account_id,
                data_connector,
                "fetch_latest_bar",
                "succeeded",
                format!(
                    "latest {} bar for {} received at {}",
                    timeframe,
                    symbol,
                    latest_bar.timestamp.to_rfc3339(),
                ),
                serde_json::json!({
                    "request": request,
                    "response": {
                        "bar": latest_bar.clone(),
                    },
                }),
            ));
            Ok((latest_bar, events))
        }
        Err(error) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.latest",
                account_id,
                data_connector,
                "fetch_latest_bar",
                "failed",
                format!("latest {timeframe} bar request failed for {symbol}"),
                serde_json::json!({
                    "request": request,
                    "error": error.to_string(),
                }),
            ));
            Err(GatewayFetchFailure {
                error: crate::connector_gateway::ConnectorGatewayRead::map_data_error(
                    lane_id, account_id, error,
                ),
                provider_events: events,
            })
        }
    }
}

pub(crate) fn gateway_fetch_latest_confirmed_bar_timestamp_with_events(
    gateway: &Gateway,
    lane_id: &str,
    account_id: &str,
    data_connector: &str,
    symbol: &str,
    timeframe: Timeframe,
) -> Result<
    (
        Option<chrono::DateTime<chrono::Utc>>,
        Vec<PendingProviderEvent>,
    ),
    GatewayFetchFailure,
> {
    validate_gateway_connector_kind(
        gateway,
        lane_id,
        account_id,
        data_connector,
        "data_connector",
    )?;
    let request = serde_json::json!({
        "symbol": symbol,
        "timeframe": timeframe,
    });
    let mut events = vec![provider_stage_event(
        lane_id,
        "provider.market_data.confirmed_target",
        account_id,
        data_connector,
        "fetch_latest_confirmed_bar_timestamp",
        "requested",
        format!("requesting latest confirmed {timeframe} bar timestamp for {symbol}"),
        serde_json::json!({
            "request": request.clone(),
        }),
    )];

    match gateway.fetch_latest_confirmed_bar_timestamp(account_id, symbol, timeframe) {
        Ok(timestamp) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.confirmed_target",
                account_id,
                data_connector,
                "fetch_latest_confirmed_bar_timestamp",
                "succeeded",
                match timestamp {
                    Some(timestamp) => format!(
                        "latest confirmed {} bar for {} is {}",
                        timeframe,
                        symbol,
                        timestamp.to_rfc3339(),
                    ),
                    None => format!("no latest confirmed {timeframe} bar available for {symbol}"),
                },
                serde_json::json!({
                    "request": request,
                    "response": {
                        "timestamp": timestamp.as_ref().map(chrono::DateTime::to_rfc3339),
                    },
                }),
            ));
            Ok((timestamp, events))
        }
        Err(error) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.confirmed_target",
                account_id,
                data_connector,
                "fetch_latest_confirmed_bar_timestamp",
                "failed",
                format!("latest confirmed {timeframe} target request failed for {symbol}"),
                serde_json::json!({
                    "request": request,
                    "error": error.to_string(),
                }),
            ));
            Err(GatewayFetchFailure {
                error: crate::connector_gateway::ConnectorGatewayRead::map_data_error(
                    lane_id, account_id, error,
                ),
                provider_events: events,
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn gateway_fetch_confirmed_bars_range_with_events(
    gateway: &Gateway,
    lane_id: &str,
    account_id: &str,
    data_connector: &str,
    symbol: &str,
    timeframe: Timeframe,
    start_after: Option<chrono::DateTime<chrono::Utc>>,
    end_at: chrono::DateTime<chrono::Utc>,
    limit: usize,
) -> Result<(ConfirmedBarPage, Vec<PendingProviderEvent>), GatewayFetchFailure> {
    validate_gateway_connector_kind(
        gateway,
        lane_id,
        account_id,
        data_connector,
        "data_connector",
    )?;
    let end_at_rfc3339 = end_at.to_rfc3339();
    let request = serde_json::json!({
        "symbol": symbol,
        "timeframe": timeframe,
        "start_after": start_after.as_ref().map(chrono::DateTime::to_rfc3339),
        "end_at": end_at_rfc3339.clone(),
        "limit": limit,
    });
    let mut events = vec![provider_stage_event(
        lane_id,
        "provider.market_data.confirmed_range",
        account_id,
        data_connector,
        "fetch_confirmed_bars_range",
        "requested",
        format!("requesting confirmed {timeframe} bars for {symbol} up to {end_at_rfc3339}"),
        serde_json::json!({
            "request": request.clone(),
        }),
    )];

    match gateway.fetch_confirmed_bars_range(
        account_id,
        symbol,
        timeframe,
        start_after,
        end_at,
        limit,
    ) {
        Ok(page) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.confirmed_range",
                account_id,
                data_connector,
                "fetch_confirmed_bars_range",
                "succeeded",
                format!(
                    "received {} confirmed {} bars for {} up to {}",
                    page.bars.len(),
                    timeframe,
                    symbol,
                    end_at_rfc3339,
                ),
                serde_json::json!({
                    "request": request,
                    "response": {
                        "returned_count": page.bars.len(),
                        "first_bar_timestamp": page.bars.first().map(|bar| bar.timestamp.to_rfc3339()),
                        "last_bar_timestamp": page.bars.last().map(|bar| bar.timestamp.to_rfc3339()),
                        "exhausted": page.exhausted,
                    },
                }),
            ));
            Ok((page, events))
        }
        Err(error) => {
            events.push(provider_stage_event(
                lane_id,
                "provider.market_data.confirmed_range",
                account_id,
                data_connector,
                "fetch_confirmed_bars_range",
                "failed",
                format!("confirmed {timeframe} range request failed for {symbol}"),
                serde_json::json!({
                    "request": request,
                    "error": error.to_string(),
                }),
            ));
            Err(GatewayFetchFailure {
                error: crate::connector_gateway::ConnectorGatewayRead::map_data_error(
                    lane_id, account_id, error,
                ),
                provider_events: events,
            })
        }
    }
}
