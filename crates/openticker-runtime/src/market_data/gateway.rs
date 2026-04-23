use crate::connector_gateway::ConnectorGatewayRead;
use crate::{
    NormalizedBarUpdate, OhlcvBar, Runtime, ServiceError, Timeframe, provider_payload_preview,
    signal_phase_label,
};
use openticker_connectors::ConfirmedBarPage;
use openticker_gateway::{Gateway, GatewayError};
use serde_json::{Map, Value};

impl ConnectorGatewayRead<'_> {
    pub(crate) fn normalize_market_stream_payload(
        &self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.market_stream",
            account_id,
            data_connector,
            "market_stream",
        );
        self.validate_connector_kind(lane_id, account_id, data_connector, "data_connector")?;
        let incoming_payload = provider_payload_preview(payload);

        provider.record_call(
            "received",
            "connector market-stream payload received",
            serde_json::json!({
                "incoming_payload": incoming_payload.clone(),
            }),
            || {
                self.gateway()
                    .normalize_market_stream_payload(account_id, payload)
            },
            |stream_update| match stream_update {
                Some(stream_update) => (
                    "normalized",
                    format!(
                        "{} {} bar normalized at {}",
                        stream_update.symbol,
                        signal_phase_label(stream_update.phase),
                        stream_update.bar.timestamp.to_rfc3339(),
                    ),
                    serde_json::json!({
                        "normalized": {
                            "symbol": stream_update.symbol.clone(),
                            "phase": signal_phase_label(stream_update.phase),
                            "bar": stream_update.bar.clone(),
                        },
                    }),
                ),
                None => (
                    "ignored",
                    "connector market-stream payload produced no normalized update".to_owned(),
                    serde_json::json!({}),
                ),
            },
            |error| {
                (
                    "connector market-stream payload failed to normalize".to_owned(),
                    serde_json::json!({
                        "error": error.to_string(),
                        "incoming_payload": incoming_payload.clone(),
                    }),
                )
            },
            |error| Self::map_data_error(lane_id, account_id, error),
        )
    }

    pub(crate) fn fetch_latest_bar(
        &self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.market_data.latest",
            account_id,
            data_connector,
            "fetch_latest_bar",
        );
        self.validate_connector_kind(lane_id, account_id, data_connector, "data_connector")?;
        let request = serde_json::json!({
            "symbol": symbol,
            "timeframe": timeframe,
        });

        provider.record_call(
            "requested",
            format!("requesting latest {timeframe} bar for {symbol}"),
            serde_json::json!({
                "request": request.clone(),
            }),
            || {
                self.gateway()
                    .fetch_latest_bar(account_id, symbol, timeframe)
            },
            |latest_bar| {
                (
                    "succeeded",
                    format!(
                        "latest {} bar for {} received at {}",
                        timeframe,
                        symbol,
                        latest_bar.timestamp.to_rfc3339(),
                    ),
                    serde_json::json!({
                        "request": request.clone(),
                        "response": {
                            "bar": latest_bar.clone(),
                        },
                    }),
                )
            },
            |error| {
                (
                    format!("latest {timeframe} bar request failed for {symbol}"),
                    serde_json::json!({
                        "request": request.clone(),
                        "error": error.to_string(),
                    }),
                )
            },
            |error| Self::map_data_error(lane_id, account_id, error),
        )
    }

    pub(crate) fn fetch_recent_bars(
        &self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.market_data.history",
            account_id,
            data_connector,
            "fetch_recent_bars",
        );
        self.validate_connector_kind(lane_id, account_id, data_connector, "data_connector")?;
        let request = serde_json::json!({
            "symbol": symbol,
            "timeframe": timeframe,
            "limit": limit,
        });

        provider.record_call(
            "requested",
            format!("requesting {limit} recent {timeframe} bars for {symbol}"),
            serde_json::json!({
                "request": request.clone(),
            }),
            || self.gateway().fetch_recent_bars(account_id, symbol, timeframe, limit),
            |bars| {
                (
                    "succeeded",
                    format!(
                        "received {} recent {} bars for {}",
                        bars.len(),
                        timeframe,
                        symbol,
                    ),
                    serde_json::json!({
                        "request": request.clone(),
                        "response": {
                            "returned_count": bars.len(),
                            "first_bar_timestamp": bars.first().map(|bar| bar.timestamp.to_rfc3339()),
                            "last_bar_timestamp": bars.last().map(|bar| bar.timestamp.to_rfc3339()),
                        },
                    }),
                )
            },
            |error| {
                (
                    format!("recent {timeframe} bar request failed for {symbol}"),
                    serde_json::json!({
                        "request": request.clone(),
                        "error": error.to_string(),
                    }),
                )
            },
            |error| Self::map_data_error(lane_id, account_id, error),
        )
    }

    pub(crate) fn fetch_recent_bars_for_stream(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ServiceError> {
        let stream_id = format!("stream:{account_id}/{symbol}/{timeframe}");
        self.gateway()
            .fetch_recent_bars(account_id, symbol, timeframe, limit)
            .map_err(|error| Self::map_data_error(&stream_id, account_id, error))
    }

    #[allow(dead_code)]
    pub(crate) fn fetch_latest_confirmed_bar_timestamp(
        &self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.market_data.confirmed_target",
            account_id,
            data_connector,
            "fetch_latest_confirmed_bar_timestamp",
        );
        self.validate_connector_kind(lane_id, account_id, data_connector, "data_connector")?;
        let request = serde_json::json!({
            "symbol": symbol,
            "timeframe": timeframe,
        });

        provider.record_call(
            "requested",
            format!("requesting latest confirmed {timeframe} bar timestamp for {symbol}"),
            serde_json::json!({
                "request": request.clone(),
            }),
            || {
                self.gateway()
                    .fetch_latest_confirmed_bar_timestamp(account_id, symbol, timeframe)
            },
            |timestamp| {
                (
                    "succeeded",
                    match timestamp {
                        Some(timestamp) => format!(
                            "latest confirmed {} bar for {} is {}",
                            timeframe,
                            symbol,
                            timestamp.to_rfc3339(),
                        ),
                        None => {
                            format!("no latest confirmed {timeframe} bar available for {symbol}")
                        }
                    },
                    serde_json::json!({
                        "request": request.clone(),
                        "response": {
                            "timestamp": timestamp.as_ref().map(chrono::DateTime::to_rfc3339),
                        },
                    }),
                )
            },
            |error| {
                (
                    format!("latest confirmed {timeframe} target request failed for {symbol}"),
                    serde_json::json!({
                        "request": request.clone(),
                        "error": error.to_string(),
                    }),
                )
            },
            |error| Self::map_data_error(lane_id, account_id, error),
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub(crate) fn fetch_confirmed_bars_range(
        &self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<chrono::DateTime<chrono::Utc>>,
        end_at: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.market_data.confirmed_range",
            account_id,
            data_connector,
            "fetch_confirmed_bars_range",
        );
        self.validate_connector_kind(lane_id, account_id, data_connector, "data_connector")?;
        let end_at_rfc3339 = end_at.to_rfc3339();
        let request = serde_json::json!({
            "symbol": symbol,
            "timeframe": timeframe,
            "start_after": start_after.as_ref().map(chrono::DateTime::to_rfc3339),
            "end_at": end_at_rfc3339.clone(),
            "limit": limit,
        });

        provider.record_call(
            "requested",
            format!("requesting confirmed {timeframe} bars for {symbol} up to {end_at_rfc3339}"),
            serde_json::json!({
                "request": request.clone(),
            }),
            || {
                self.gateway().fetch_confirmed_bars_range(
                    account_id,
                    symbol,
                    timeframe,
                    start_after,
                    end_at,
                    limit,
                )
            },
            |page| {
                (
                    "succeeded",
                    format!(
                        "received {} confirmed {} bars for {} up to {}",
                        page.bars.len(),
                        timeframe,
                        symbol,
                        end_at_rfc3339,
                    ),
                    serde_json::json!({
                        "request": request.clone(),
                        "response": {
                            "returned_count": page.bars.len(),
                            "first_bar_timestamp": page.bars.first().map(|bar| bar.timestamp.to_rfc3339()),
                            "last_bar_timestamp": page.bars.last().map(|bar| bar.timestamp.to_rfc3339()),
                            "exhausted": page.exhausted,
                        },
                    }),
                )
            },
            |error| {
                (
                    format!("confirmed {timeframe} range request failed for {symbol}"),
                    serde_json::json!({
                        "request": request.clone(),
                        "error": error.to_string(),
                    }),
                )
            },
            |error| Self::map_data_error(lane_id, account_id, error),
        )
    }
}

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
