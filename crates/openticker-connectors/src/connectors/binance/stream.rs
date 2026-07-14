use super::BinanceConnector;
use super::de::{
    deserialize_f64_from_string_or_number, deserialize_option_f64_from_string_or_number,
};
use super::klines::binance_interval_label;
use crate::{
    ConnectorAccount, ConnectorError, ConnectorKind, ConnectorMarketStreamSubscription,
    ConnectorPreviewStreamCommand, ConnectorPreviewStreamEvent, ConnectorPrivateAccountEvent,
    ConnectorPrivateBalance, ConnectorPrivateStreamEvent, PreviewStreamConnectionState,
};
use futures_util::{SinkExt, StreamExt};
use openticker_core::{OhlcvBar, SignalPhase, Timeframe};
use openticker_data::{NormalizedBarUpdate, NormalizedOrderEvent};
use serde::Deserialize;
use std::collections::BTreeSet;
use tokio::sync::mpsc;
use tokio::time::{Duration as TokioDuration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const BINANCE_MARKET_STREAM_WS_URL: &str = "wss://stream.binance.com:9443/ws";

#[derive(Debug, Deserialize)]
struct BinanceCombinedMarketEvent {
    stream: Option<String>,
    data: BinanceKlineEvent,
}

#[derive(Debug, Deserialize)]
struct BinanceStreamControlEvent {
    result: Option<serde_json::Value>,
    id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct BinanceKlineEvent {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: BinanceKlinePayload,
}

#[derive(Debug, Clone, Deserialize)]
struct BinanceKlinePayload {
    #[serde(rename = "t")]
    open_time_ms: i64,
    #[serde(
        rename = "o",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    open: f64,
    #[serde(
        rename = "h",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    high: f64,
    #[serde(
        rename = "l",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    low: f64,
    #[serde(
        rename = "c",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    close: f64,
    #[serde(
        rename = "v",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    volume: f64,
    #[serde(rename = "i")]
    interval: Option<String>,
    #[serde(rename = "x")]
    is_closed: bool,
}

#[derive(Debug, Deserialize)]
struct BinanceCombinedPrivateEvent {
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct BinancePrivateEventEnvelope {
    #[serde(rename = "e")]
    event_type: String,
}

#[derive(Debug, Deserialize)]
struct BinanceExecutionReportPayload {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "c")]
    client_order_id: String,
    #[serde(rename = "X")]
    status: String,
    #[serde(rename = "S")]
    side: String,
    #[serde(
        rename = "q",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    order_quantity: f64,
    #[serde(
        rename = "z",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    cumulative_filled_quantity: f64,
    #[serde(
        rename = "L",
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    last_fill_price: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct BinanceOutboundAccountPositionPayload {
    #[serde(rename = "B")]
    balances: Vec<BinancePrivateBalancePayload>,
}

#[derive(Debug, Deserialize)]
struct BinancePrivateBalancePayload {
    #[serde(rename = "a")]
    asset: String,
    #[serde(
        rename = "f",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    free: f64,
    #[serde(
        rename = "l",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    locked: f64,
}

pub(super) fn normalize_market_data_event(
    payload: &str,
) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
    if payload.trim().is_empty() {
        return Ok(None);
    }

    if let Ok(combined) = serde_json::from_str::<BinanceCombinedMarketEvent>(payload) {
        return normalize_kline_event(combined.data);
    }
    if let Ok(raw) = serde_json::from_str::<BinanceKlineEvent>(payload) {
        return normalize_kline_event(raw);
    }

    Err(ConnectorError::StreamDecode {
        kind: ConnectorKind::Binance,
        detail: "payload did not match Binance kline websocket schema".to_owned(),
    })
}

fn normalize_market_data_event_with_subscription(
    payload: &str,
) -> Result<Option<(ConnectorMarketStreamSubscription, NormalizedBarUpdate)>, ConnectorError> {
    if payload.trim().is_empty() {
        return Ok(None);
    }

    if let Ok(control) = serde_json::from_str::<BinanceStreamControlEvent>(payload)
        && (control.result.is_some() || control.id.is_some())
    {
        return Ok(None);
    }

    if let Ok(combined) = serde_json::from_str::<BinanceCombinedMarketEvent>(payload) {
        return normalize_kline_event_with_subscription(combined.data, combined.stream.as_deref());
    }
    if let Ok(raw) = serde_json::from_str::<BinanceKlineEvent>(payload) {
        return normalize_kline_event_with_subscription(raw, None);
    }

    Err(ConnectorError::StreamDecode {
        kind: ConnectorKind::Binance,
        detail: "payload did not match Binance kline websocket schema".to_owned(),
    })
}

fn normalize_kline_event(
    event: BinanceKlineEvent,
) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
    if event.event_type != "kline" {
        return Ok(None);
    }

    let timestamp =
        chrono::DateTime::from_timestamp_millis(event.kline.open_time_ms).ok_or_else(|| {
            ConnectorError::StreamDecode {
                kind: ConnectorKind::Binance,
                detail: format!(
                    "kline open time `{}` is not a valid timestamp",
                    event.kline.open_time_ms
                ),
            }
        })?;
    let phase = if event.kline.is_closed {
        SignalPhase::Confirmed
    } else {
        SignalPhase::Preview
    };

    Ok(Some(NormalizedBarUpdate {
        symbol: event.symbol,
        bar: OhlcvBar {
            timestamp,
            open: event.kline.open,
            high: event.kline.high,
            low: event.kline.low,
            close: event.kline.close,
            volume: event.kline.volume,
        },
        phase,
    }))
}

fn normalize_kline_event_with_subscription(
    event: BinanceKlineEvent,
    stream_name: Option<&str>,
) -> Result<Option<(ConnectorMarketStreamSubscription, NormalizedBarUpdate)>, ConnectorError> {
    let interval = event
        .kline
        .interval
        .as_deref()
        .or_else(|| stream_name.and_then(binance_interval_from_stream_name))
        .ok_or_else(|| ConnectorError::StreamDecode {
            kind: ConnectorKind::Binance,
            detail: "Binance kline payload did not include an interval".to_owned(),
        })?;
    let timeframe = binance_timeframe_from_interval(interval)?;
    normalize_kline_event(event).map(|update| {
        update.map(|update| {
            (
                ConnectorMarketStreamSubscription {
                    symbol: update.symbol.clone(),
                    timeframe,
                },
                update,
            )
        })
    })
}

fn binance_interval_from_stream_name(stream_name: &str) -> Option<&str> {
    stream_name
        .rsplit_once("@kline_")
        .map(|(_, interval)| interval)
}

fn binance_timeframe_from_interval(interval: &str) -> Result<Timeframe, ConnectorError> {
    match interval {
        "1m" => Ok(Timeframe::M1),
        "5m" => Ok(Timeframe::M5),
        "15m" => Ok(Timeframe::M15),
        "30m" => Ok(Timeframe::M30),
        "1h" => Ok(Timeframe::H1),
        "4h" => Ok(Timeframe::H4),
        "1d" => Ok(Timeframe::D1),
        _ => Err(ConnectorError::StreamDecode {
            kind: ConnectorKind::Binance,
            detail: format!("unsupported Binance kline interval `{interval}`"),
        }),
    }
}

fn binance_stream_name(subscription: &ConnectorMarketStreamSubscription) -> String {
    format!(
        "{}@kline_{}",
        subscription.symbol.to_lowercase(),
        binance_interval_label(subscription.timeframe)
    )
}

async fn send_binance_subscription_change<S>(
    stream: &mut S,
    method: &str,
    subscriptions: &BTreeSet<ConnectorMarketStreamSubscription>,
    request_id: &mut u64,
) -> Result<(), ConnectorError>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    if subscriptions.is_empty() {
        return Ok(());
    }

    let params = subscriptions
        .iter()
        .map(binance_stream_name)
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "method": method,
        "params": params,
        "id": *request_id,
    });
    *request_id = request_id.saturating_add(1);

    stream
        .send(Message::Text(payload.to_string()))
        .await
        .map_err(|error| ConnectorError::PreviewStream {
            kind: ConnectorKind::Binance,
            detail: format!("failed to send Binance preview-stream {method} request: {error}"),
        })
}

/// Emits a preview-stream event without blocking the worker.
///
/// Backpressure policy: the event channel is bounded
/// ([`PREVIEW_STREAM_EVENT_CAPACITY`]). On a full channel the event is dropped
/// (with a `warn!`) rather than awaited; a stalled consumer therefore loses the
/// freshest preview update — acceptable for a best-effort market-data preview —
/// instead of letting the queue grow without bound and risk OOM. A closed
/// channel (consumer gone) is silently ignored.
fn emit_preview_event(
    event_tx: &mpsc::Sender<ConnectorPreviewStreamEvent>,
    account_id: &str,
    event: ConnectorPreviewStreamEvent,
) {
    if let Err(error) = event_tx.try_send(event) {
        match error {
            mpsc::error::TrySendError::Full(_) => {
                tracing::warn!(
                    account_id,
                    "binance preview stream event channel full; dropping event"
                );
            }
            mpsc::error::TrySendError::Closed(_) => {}
        }
    }
}

#[allow(clippy::too_many_lines)]
pub(super) async fn run_binance_preview_stream_worker(
    account: ConnectorAccount,
    mut command_rx: mpsc::Receiver<ConnectorPreviewStreamCommand>,
    event_tx: mpsc::Sender<ConnectorPreviewStreamEvent>,
) {
    let mut desired = BTreeSet::new();
    let resilience_policy = BinanceConnector::resilience_policy();
    let mut reconnect_failures = 0u32;
    let mut request_id = 1u64;

    loop {
        while desired.is_empty() {
            match command_rx.recv().await {
                Some(ConnectorPreviewStreamCommand::ReplaceSubscriptions(subscriptions)) => {
                    desired = subscriptions.into_iter().collect();
                }
                Some(ConnectorPreviewStreamCommand::Shutdown) | None => return,
            }
        }

        emit_preview_event(
            &event_tx,
            &account.account_id,
            ConnectorPreviewStreamEvent::ConnectionState {
                state: PreviewStreamConnectionState::Connecting,
                detail: None,
            },
        );

        match connect_async(BINANCE_MARKET_STREAM_WS_URL).await {
            Ok((mut socket, _)) => {
                reconnect_failures = 0;
                emit_preview_event(
                    &event_tx,
                    &account.account_id,
                    ConnectorPreviewStreamEvent::ConnectionState {
                        state: PreviewStreamConnectionState::Connected,
                        detail: Some(format!("account={}", account.account_id)),
                    },
                );

                if let Err(error) = send_binance_subscription_change(
                    &mut socket,
                    "SUBSCRIBE",
                    &desired,
                    &mut request_id,
                )
                .await
                {
                    emit_preview_event(
                        &event_tx,
                        &account.account_id,
                        ConnectorPreviewStreamEvent::ConnectionState {
                            state: PreviewStreamConnectionState::Disconnected,
                            detail: Some(error.to_string()),
                        },
                    );
                } else {
                    let mut active = desired.clone();
                    let mut reconnect = false;
                    while !reconnect {
                        tokio::select! {
                            command = command_rx.recv() => {
                                match command {
                                    Some(ConnectorPreviewStreamCommand::ReplaceSubscriptions(subscriptions)) => {
                                        let next = subscriptions.into_iter().collect::<BTreeSet<_>>();
                                        let to_subscribe = next.difference(&active).cloned().collect::<BTreeSet<_>>();
                                        let to_unsubscribe = active.difference(&next).cloned().collect::<BTreeSet<_>>();

                                        if let Err(error) = send_binance_subscription_change(&mut socket, "UNSUBSCRIBE", &to_unsubscribe, &mut request_id).await {
                                            emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                                state: PreviewStreamConnectionState::Disconnected,
                                                detail: Some(error.to_string()),
                                            });
                                            reconnect = true;
                                            desired = next;
                                            continue;
                                        }
                                        if let Err(error) = send_binance_subscription_change(&mut socket, "SUBSCRIBE", &to_subscribe, &mut request_id).await {
                                            emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                                state: PreviewStreamConnectionState::Disconnected,
                                                detail: Some(error.to_string()),
                                            });
                                            reconnect = true;
                                            desired = next;
                                            continue;
                                        }

                                        desired = next.clone();
                                        active = next;
                                        if active.is_empty() {
                                            let _ = socket.close(None).await;
                                            emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                                state: PreviewStreamConnectionState::Disconnected,
                                                detail: Some("preview stream idle".to_owned()),
                                            });
                                            break;
                                        }
                                    }
                                    Some(ConnectorPreviewStreamCommand::Shutdown) | None => {
                                        let _ = socket.close(None).await;
                                        return;
                                    }
                                }
                            }
                            message = socket.next() => {
                                match message {
                                    Some(Ok(Message::Text(payload))) => {
                                        match normalize_market_data_event_with_subscription(payload.as_str()) {
                                            Ok(Some((subscription, update))) => {
                                                emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::BarUpdate {
                                                    subscription,
                                                    update,
                                                });
                                            }
                                            Ok(None) => {}
                                            Err(error) => {
                                                emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                                    state: PreviewStreamConnectionState::Disconnected,
                                                    detail: Some(error.to_string()),
                                                });
                                                reconnect = true;
                                            }
                                        }
                                    }
                                    Some(Ok(Message::Ping(payload))) => {
                                        let _ = socket.send(Message::Pong(payload)).await;
                                    }
                                    Some(Ok(Message::Close(frame))) => {
                                        let detail = frame.map_or_else(
                                            || "Binance preview stream closed".to_owned(),
                                            |frame| format!("Binance preview stream closed: {}", frame.reason),
                                        );
                                        emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                            state: PreviewStreamConnectionState::Disconnected,
                                            detail: Some(detail),
                                        });
                                        reconnect = true;
                                    }
                                    Some(Ok(_)) => {}
                                    Some(Err(error)) => {
                                        emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                            state: PreviewStreamConnectionState::Disconnected,
                                            detail: Some(format!("Binance preview stream error: {error}")),
                                        });
                                        reconnect = true;
                                    }
                                    None => {
                                        emit_preview_event(&event_tx, &account.account_id, ConnectorPreviewStreamEvent::ConnectionState {
                                            state: PreviewStreamConnectionState::Disconnected,
                                            detail: Some("Binance preview stream ended".to_owned()),
                                        });
                                        reconnect = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(error) => {
                emit_preview_event(
                    &event_tx,
                    &account.account_id,
                    ConnectorPreviewStreamEvent::ConnectionState {
                        state: PreviewStreamConnectionState::Disconnected,
                        detail: Some(format!("failed to connect Binance preview stream: {error}")),
                    },
                );
            }
        }

        reconnect_failures = reconnect_failures.saturating_add(1);
        let delay_ms = resilience_policy.reconnect_delay_ms(reconnect_failures.max(1));
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(ConnectorPreviewStreamCommand::ReplaceSubscriptions(subscriptions)) => {
                        desired = subscriptions.into_iter().collect();
                    }
                    Some(ConnectorPreviewStreamCommand::Shutdown) | None => return,
                }
            }
            () = sleep(TokioDuration::from_millis(delay_ms)) => {}
        }
    }
}

pub(super) fn normalize_private_event(
    payload: &str,
) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
    if payload.trim().is_empty() {
        return Ok(None);
    }

    if let Ok(combined) = serde_json::from_str::<BinanceCombinedPrivateEvent>(payload) {
        return normalize_private_event_value(combined.data);
    }

    let raw = serde_json::from_str::<serde_json::Value>(payload).map_err(|error| {
        ConnectorError::StreamDecode {
            kind: ConnectorKind::Binance,
            detail: format!("failed to decode private stream payload JSON: {error}"),
        }
    })?;
    normalize_private_event_value(raw)
}

fn normalize_private_event_value(
    payload: serde_json::Value,
) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
    let event_type = serde_json::from_value::<BinancePrivateEventEnvelope>(payload.clone())
        .map_err(|error| ConnectorError::StreamDecode {
            kind: ConnectorKind::Binance,
            detail: format!("payload did not include Binance private event type: {error}"),
        })?
        .event_type;

    match event_type.as_str() {
        "executionReport" => {
            let event = serde_json::from_value::<BinanceExecutionReportPayload>(payload).map_err(
                |error| ConnectorError::StreamDecode {
                    kind: ConnectorKind::Binance,
                    detail: format!(
                        "payload did not match Binance executionReport schema: {error}"
                    ),
                },
            )?;
            Ok(Some(ConnectorPrivateStreamEvent::Order(
                NormalizedOrderEvent {
                    symbol: event.symbol,
                    client_order_id: event.client_order_id,
                    status: event.status,
                    side: event.side,
                    order_quantity: event.order_quantity,
                    cumulative_filled_quantity: event.cumulative_filled_quantity,
                    last_fill_price: event.last_fill_price,
                },
            )))
        }
        "outboundAccountPosition" => {
            let event = serde_json::from_value::<BinanceOutboundAccountPositionPayload>(payload)
                .map_err(|error| ConnectorError::StreamDecode {
                    kind: ConnectorKind::Binance,
                    detail: format!(
                        "payload did not match Binance outboundAccountPosition schema: {error}"
                    ),
                })?;
            let balances = event
                .balances
                .into_iter()
                .map(|balance| ConnectorPrivateBalance {
                    asset: balance.asset,
                    free: balance.free,
                    locked: balance.locked,
                })
                .collect();
            Ok(Some(ConnectorPrivateStreamEvent::Account(
                ConnectorPrivateAccountEvent { balances },
            )))
        }
        _ => Ok(None),
    }
}
