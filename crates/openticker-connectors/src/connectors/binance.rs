use crate::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot, ConnectorError,
    ConnectorExecution, ConnectorHealth, ConnectorKind, ConnectorMarketData, ConnectorMarketStream,
    ConnectorMarketStreamSubscription, ConnectorOpenOrder, ConnectorPosition,
    ConnectorPreviewStreamCommand, ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession,
    ConnectorPrivateAccountEvent, ConnectorPrivateBalance, ConnectorPrivateStream,
    ConnectorPrivateStreamEvent, ConnectorReconcile, ConnectorResiliencePolicy,
    ConnectorResilienceState, ConnectorRuntimeControl, ConnectorStatus, ConnectorSymbolConstraints,
    ConnectorSymbolConstraintsLookup, PreviewStreamConnectionState, StubConnector,
    default_blocking_http_client, descriptor_for, deterministic_remote_client_order_id,
    resolve_secret_env_value, run_in_blocking_thread, unix_now_ms,
};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use openticker_core::{ExecutionMode, OhlcvBar, SignalPhase, Timeframe};
use openticker_data::{NormalizedBarUpdate, NormalizedOrderEvent};
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, ExecutionRouter, OrderSide, OrderType, PaperExecutionRouter,
};
use reqwest::blocking::Response;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use sha2::Sha256;
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Duration as TokioDuration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

type HmacSha256 = Hmac<Sha256>;

const BINANCE_LIVE_BASE_URL: &str = "https://api.binance.com";
const BINANCE_DEMO_BASE_URL: &str = "https://demo-api.binance.com";
const BINANCE_ORDER_STATUS_POLL_ATTEMPTS: u8 = 20;
const BINANCE_ORDER_STATUS_POLL_INTERVAL_MS: u64 = 250;
const BINANCE_MAX_QUANTITY_DECIMALS: usize = 8;
const BINANCE_MAX_QUANTITY_DECIMALS_I32: i32 = 8;
const BINANCE_QUANTITY_FLOOR_TOLERANCE: f64 = 1e-6;
const BINANCE_MARKET_STREAM_WS_URL: &str = "wss://stream.binance.com:9443/ws";

#[derive(Debug, Clone)]
pub struct BinanceConnector {
    inner: StubConnector,
    account: ConnectorAccount,
    resilience: ConnectorResilienceState,
}

impl BinanceConnector {
    #[must_use]
    pub fn new(account: &ConnectorAccount) -> Self {
        Self {
            inner: StubConnector::new(ConnectorKind::Binance, account.mode, account.use_demo_mode),
            account: account.clone(),
            resilience: ConnectorResilienceState::default(),
        }
    }

    #[must_use]
    pub fn resilience_policy() -> ConnectorResiliencePolicy {
        descriptor_for(ConnectorKind::Binance).resilience
    }

    #[must_use]
    pub fn resilience_state(&self) -> ConnectorResilienceState {
        self.resilience
    }

    pub fn note_disconnect(&mut self, now_ms: i64) {
        self.resilience
            .note_disconnect(now_ms, Self::resilience_policy());
    }

    pub fn note_reconnect_success(&mut self) {
        self.resilience.note_reconnect_success();
    }

    pub fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64) {
        self.resilience.note_rate_limit(now_ms, throttle_window_ms);
    }

    pub fn set_state(&mut self, state: ConnectionState) {
        self.inner.set_state(state);
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the selected execution mode is unsupported.
    pub fn validate_mode(&self) -> Result<(), ConnectorError> {
        self.inner.validate_mode()
    }

    fn fetch_remote_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        let api_key = resolve_secret_env_value(
            ConnectorKind::Binance,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Binance,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();

        run_in_blocking_thread(
            ConnectorKind::Binance,
            "binance-remote-snapshot",
            move || {
                let timestamp = unix_now_ms();
                let open_orders_query = format!("recvWindow=5000&timestamp={timestamp}");
                let open_orders_signature = sign_query(&api_secret, &open_orders_query)?;
                let open_orders_url = format!(
                    "{base_url}/api/v3/openOrders?{open_orders_query}&signature={open_orders_signature}"
                );
                let open_orders_response = client
                    .get(open_orders_url)
                    .header("X-MBX-APIKEY", api_key.as_str())
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Binance,
                        detail: format!("open orders request failed: {error}"),
                    })?;
                let open_orders_payload: Vec<BinanceOpenOrderPayload> =
                    decode_json_response(open_orders_response, "open orders")?;

                let account_query = format!("recvWindow=5000&timestamp={timestamp}");
                let account_signature = sign_query(&api_secret, &account_query)?;
                let account_url = format!(
                    "{base_url}/api/v3/account?{account_query}&signature={account_signature}"
                );
                let account_response = client
                    .get(account_url)
                    .header("X-MBX-APIKEY", api_key.as_str())
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Binance,
                        detail: format!("account request failed: {error}"),
                    })?;
                let account_payload: BinanceAccountPayload =
                    decode_json_response(account_response, "account")?;

                Ok(ConnectorAccountSnapshot {
                    open_orders: normalize_orders(open_orders_payload),
                    positions: normalize_positions(account_payload.balances.clone()),
                    balances: normalize_balances(account_payload.balances),
                })
            },
        )
    }

    fn reconciliation_base_url(&self) -> String {
        self.account
            .reconciliation_base_url
            .clone()
            .unwrap_or_else(|| {
                let use_demo_endpoint =
                    self.account.mode == ExecutionMode::Paper || self.account.use_demo_mode;
                if use_demo_endpoint {
                    BINANCE_DEMO_BASE_URL
                } else {
                    BINANCE_LIVE_BASE_URL
                }
                .to_owned()
            })
            .trim_end_matches('/')
            .to_owned()
    }

    fn fetch_remote_latest_bar(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError> {
        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();
        let interval = binance_interval_label(timeframe);
        let symbol = symbol.to_owned();

        run_in_blocking_thread(ConnectorKind::Binance, "binance-latest-bar", move || {
            let now_ms = unix_now_ms();
            let response = client
                .get(format!("{base_url}/api/v3/klines"))
                .query(&[
                    ("symbol", symbol.as_str()),
                    ("interval", interval),
                    ("limit", "2"),
                ])
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("latest kline request failed: {error}"),
                })?;
            let rows: Vec<Vec<serde_json::Value>> = decode_json_response(response, "latest kline")?;

            latest_confirmed_binance_bar(rows, now_ms)?.ok_or_else(|| {
                ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!(
                        "latest kline response did not include confirmed data for `{symbol}`"
                    ),
                }
            })
        })
    }

    fn fetch_remote_recent_bars(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();
        let interval = binance_interval_label(timeframe);
        let symbol = symbol.to_owned();
        let requested = limit.saturating_add(1).min(1_000);

        run_in_blocking_thread(ConnectorKind::Binance, "binance-recent-bars", move || {
            let response = client
                .get(format!("{base_url}/api/v3/klines"))
                .query(&[
                    ("symbol", symbol.as_str()),
                    ("interval", interval),
                    ("limit", requested.to_string().as_str()),
                ])
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("recent kline request failed: {error}"),
                })?;
            let rows: Vec<Vec<serde_json::Value>> = decode_json_response(response, "recent kline")?;

            normalize_recent_binance_klines(rows, limit, unix_now_ms())
        })
    }

    fn fetch_remote_latest_confirmed_bar_timestamp(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, ConnectorError> {
        self.fetch_remote_latest_bar(symbol, timeframe)
            .map(|bar| Some(bar.timestamp))
    }

    fn fetch_remote_confirmed_bars_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<chrono::DateTime<chrono::Utc>>,
        end_at: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ConnectorError> {
        if limit == 0 {
            return Ok(ConfirmedBarPage {
                bars: Vec::new(),
                exhausted: false,
            });
        }

        if start_after.is_some_and(|start| start >= end_at) {
            return Ok(ConfirmedBarPage {
                bars: Vec::new(),
                exhausted: true,
            });
        }

        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();
        let interval = binance_interval_label(timeframe);
        let symbol = symbol.to_owned();
        let requested = limit.clamp(1, 1_000);
        let requested_string = requested.to_string();
        let end_time_ms = end_at.timestamp_millis();
        let timeframe_ms = i64::try_from(timeframe.duration().as_millis())
            .unwrap_or(i64::MAX)
            .max(1);
        let request_end_time = end_time_ms
            .saturating_add(timeframe_ms.saturating_sub(1))
            .to_string();
        let request_start_time =
            start_after.map(|timestamp| timestamp.timestamp_millis().saturating_add(1).to_string());

        run_in_blocking_thread(
            ConnectorKind::Binance,
            "binance-confirmed-bars-range",
            move || {
                let mut request = client.get(format!("{base_url}/api/v3/klines")).query(&[
                    ("symbol", symbol.as_str()),
                    ("interval", interval),
                    ("limit", requested_string.as_str()),
                    ("endTime", request_end_time.as_str()),
                ]);

                if let Some(start_time) = request_start_time.as_deref() {
                    request = request.query(&[("startTime", start_time)]);
                }

                let response = request
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Binance,
                        detail: format!("confirmed range request failed: {error}"),
                    })?;
                let rows: Vec<Vec<serde_json::Value>> =
                    decode_json_response(response, "confirmed range")?;

                let now_ms = unix_now_ms();
                let mut bars = rows
                    .into_iter()
                    .filter_map(|row| match parse_kline_row_with_close_time(&row) {
                        Ok((bar, close_time_ms))
                            if close_time_ms <= now_ms
                                && bar.timestamp <= end_at
                                && start_after.is_none_or(|start| bar.timestamp > start) =>
                        {
                            Some(Ok(bar))
                        }
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                bars.sort_by_key(|bar| bar.timestamp);

                Ok(ConfirmedBarPage {
                    exhausted: bars.last().is_some_and(|bar| bar.timestamp >= end_at)
                        || bars.len() < requested,
                    bars,
                })
            },
        )
    }

    fn fetch_remote_symbol_constraints(
        &self,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();
        let symbol = symbol.to_owned();

        run_in_blocking_thread(
            ConnectorKind::Binance,
            "binance-symbol-constraints",
            move || {
                let response = client
                    .get(format!("{base_url}/api/v3/exchangeInfo"))
                    .query(&[("symbol", symbol.as_str())])
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Binance,
                        detail: format!("exchangeInfo request failed: {error}"),
                    })?;
                let payload: BinanceExchangeInfoPayload =
                    decode_json_response(response, "exchangeInfo")?;

                extract_symbol_constraints(payload, &symbol)
            },
        )
    }

    fn submit_remote_order(
        &self,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, ConnectorError> {
        let accepted_stub = PaperExecutionRouter.submit(request).map_err(|error| {
            ConnectorError::OrderSubmission {
                kind: ConnectorKind::Binance,
                detail: error.to_string(),
            }
        })?;
        let api_key = resolve_secret_env_value(
            ConnectorKind::Binance,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Binance,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Binance)?;
        let base_url = self.reconciliation_base_url();
        let symbol = request.symbol.clone();
        let request_price = request.price;
        let request_quantity = request.quantity;
        let side = accepted_stub.side;
        let client_order_id = deterministic_remote_client_order_id(ConnectorKind::Binance, request);

        run_in_blocking_thread(ConnectorKind::Binance, "binance-submit-order", move || {
            let side_label = binance_order_side_label(side);
            let quantity = format_binance_quantity(request_quantity);
            let mut order_payload = submit_binance_market_order(
                &client,
                base_url.as_str(),
                api_key.as_str(),
                api_secret.as_str(),
                symbol.as_str(),
                side_label,
                quantity.as_str(),
                client_order_id.as_str(),
            )?;

            for attempt in 0..=BINANCE_ORDER_STATUS_POLL_ATTEMPTS {
                if binance_order_status_is_terminal(order_payload.status.as_str()) {
                    if let Some(accepted) =
                        accepted_order_from_binance_payload(&order_payload, side, request_price)
                    {
                        return Ok(accepted);
                    }
                    return Err(ConnectorError::OrderSubmission {
                        kind: ConnectorKind::Binance,
                        detail: format!(
                            "order `{}` reached terminal status `{}` without any fill quantity",
                            order_payload.client_order_id, order_payload.status
                        ),
                    });
                }

                if attempt == BINANCE_ORDER_STATUS_POLL_ATTEMPTS {
                    break;
                }

                std::thread::sleep(Duration::from_millis(BINANCE_ORDER_STATUS_POLL_INTERVAL_MS));
                order_payload = fetch_binance_order_status(
                    &client,
                    base_url.as_str(),
                    api_key.as_str(),
                    api_secret.as_str(),
                    symbol.as_str(),
                    order_payload.client_order_id.as_str(),
                )?;
            }

            Err(ConnectorError::OrderSubmission {
                kind: ConnectorKind::Binance,
                detail: format!(
                    "order `{}` did not reach terminal status within {} polling attempts (latest status `{}`)",
                    order_payload.client_order_id,
                    BINANCE_ORDER_STATUS_POLL_ATTEMPTS,
                    order_payload.status
                ),
            })
        })
    }
}

impl ConnectorHealth for BinanceConnector {
    fn health(&self) -> ConnectorStatus {
        self.inner.health()
    }

    fn resilience_policy(&self) -> ConnectorResiliencePolicy {
        Self::resilience_policy()
    }

    fn resilience_state(&self) -> ConnectorResilienceState {
        self.resilience
    }
}

impl ConnectorReconcile for BinanceConnector {
    fn fetch_account_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self.inner.fetch_account_snapshot();
        }
        self.fetch_remote_snapshot()
    }
}

impl ConnectorMarketData for BinanceConnector {
    fn fetch_latest_bar(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self.inner.fetch_latest_bar(symbol, timeframe);
        }
        self.fetch_remote_latest_bar(symbol, timeframe)
    }

    fn fetch_recent_bars(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self.inner.fetch_recent_bars(symbol, timeframe, limit);
        }
        self.fetch_remote_recent_bars(symbol, timeframe, limit)
    }

    fn fetch_latest_confirmed_bar_timestamp(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self
                .inner
                .fetch_latest_confirmed_bar_timestamp(symbol, timeframe);
        }
        self.fetch_remote_latest_confirmed_bar_timestamp(symbol, timeframe)
    }

    fn fetch_confirmed_bars_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<chrono::DateTime<chrono::Utc>>,
        end_at: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self.inner.fetch_confirmed_bars_range(
                symbol,
                timeframe,
                start_after,
                end_at,
                limit,
            );
        }
        self.fetch_remote_confirmed_bars_range(symbol, timeframe, start_after, end_at, limit)
    }
}

impl ConnectorExecution for BinanceConnector {
    fn submit_order(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ConnectorError> {
        if !self.account.execution_remote_submission {
            return self.inner.submit_order(request);
        }
        self.submit_remote_order(request)
    }
}

impl ConnectorSymbolConstraintsLookup for BinanceConnector {
    fn fetch_symbol_constraints(
        &self,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return Ok(ConnectorSymbolConstraints::default());
        }

        self.fetch_remote_symbol_constraints(symbol)
    }
}

impl ConnectorMarketStream for BinanceConnector {
    fn normalize_market_data_event(
        &self,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
        normalize_market_data_event(payload)
    }

    fn start_preview_stream_session(
        &self,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let account = self.account.clone();
        tokio::spawn(run_binance_preview_stream_worker(
            account, command_rx, event_tx,
        ));
        Ok(Some(ConnectorPreviewStreamSession::new(
            command_tx, event_rx,
        )))
    }
}

impl ConnectorPrivateStream for BinanceConnector {
    fn normalize_private_stream_event(
        &self,
        payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
        normalize_private_event(payload)
    }
}

impl ConnectorRuntimeControl for BinanceConnector {
    fn note_disconnect(&mut self, now_ms: i64) {
        BinanceConnector::note_disconnect(self, now_ms);
    }

    fn note_reconnect_success(&mut self) {
        BinanceConnector::note_reconnect_success(self);
    }

    fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64) {
        BinanceConnector::note_rate_limit(self, now_ms, throttle_window_ms);
    }
}

#[derive(Debug, Deserialize)]
struct BinanceOpenOrderPayload {
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    symbol: String,
    status: String,
    #[serde(
        rename = "origQty",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    orig_qty: f64,
}

#[derive(Debug, Deserialize)]
struct BinanceAccountPayload {
    balances: Vec<BinanceBalancePayload>,
}

#[derive(Debug, Deserialize)]
struct BinanceExchangeInfoPayload {
    #[serde(default)]
    symbols: Vec<BinanceExchangeSymbolPayload>,
}

#[derive(Debug, Deserialize)]
struct BinanceExchangeSymbolPayload {
    symbol: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    filters: Vec<BinanceExchangeFilterPayload>,
}

#[derive(Debug, Deserialize)]
struct BinanceExchangeFilterPayload {
    #[serde(rename = "filterType")]
    filter_type: String,
    #[serde(rename = "minQty", default)]
    min_qty: Option<String>,
    #[serde(rename = "stepSize", default)]
    step_size: Option<String>,
    #[serde(rename = "minNotional", default)]
    min_notional: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BinanceBalancePayload {
    asset: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    free: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    locked: f64,
}

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

#[derive(Debug, Deserialize)]
struct BinanceSubmittedOrderPayload {
    symbol: String,
    #[serde(rename = "clientOrderId")]
    client_order_id: String,
    status: String,
    #[serde(
        rename = "executedQty",
        deserialize_with = "deserialize_f64_from_string_or_number"
    )]
    executed_qty: f64,
    #[serde(
        rename = "cummulativeQuoteQty",
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    cumulative_quote_qty: Option<f64>,
    #[serde(default)]
    fills: Vec<BinanceSubmittedOrderFillPayload>,
}

#[derive(Debug, Deserialize)]
struct BinanceSubmittedOrderFillPayload {
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    price: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    commission: Option<f64>,
    #[serde(rename = "commissionAsset", default)]
    commission_asset: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn submit_binance_market_order(
    client: &reqwest::blocking::Client,
    base_url: &str,
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    side: &str,
    quantity: &str,
    client_order_id: &str,
) -> Result<BinanceSubmittedOrderPayload, ConnectorError> {
    let timestamp = unix_now_ms();
    let query = format!(
        "symbol={symbol}&side={side}&type=MARKET&quantity={quantity}&newClientOrderId={client_order_id}&newOrderRespType=FULL&recvWindow=5000&timestamp={timestamp}"
    );
    let signature = sign_query(api_secret, &query)?;
    let response = client
        .post(format!(
            "{base_url}/api/v3/order?{query}&signature={signature}"
        ))
        .header("X-MBX-APIKEY", api_key)
        .send()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("order submission request failed: {error}"),
        })?;
    decode_order_submission_json(response, "submit order")
}

fn format_binance_quantity(value: f64) -> String {
    if !value.is_finite() || value <= 0.0 {
        return "0".to_owned();
    }

    let scale = 10_f64.powi(BINANCE_MAX_QUANTITY_DECIMALS_I32);
    let floored = ((value * scale) + BINANCE_QUANTITY_FLOOR_TOLERANCE).floor() / scale;
    let precision = BINANCE_MAX_QUANTITY_DECIMALS;
    let mut formatted = format!("{floored:.precision$}");
    while formatted.contains('.') && formatted.ends_with('0') {
        formatted.pop();
    }
    if formatted.ends_with('.') {
        formatted.pop();
    }
    formatted
}

fn fetch_binance_order_status(
    client: &reqwest::blocking::Client,
    base_url: &str,
    api_key: &str,
    api_secret: &str,
    symbol: &str,
    client_order_id: &str,
) -> Result<BinanceSubmittedOrderPayload, ConnectorError> {
    let timestamp = unix_now_ms();
    let query = format!(
        "symbol={symbol}&origClientOrderId={client_order_id}&recvWindow=5000&timestamp={timestamp}"
    );
    let signature = sign_query(api_secret, &query)?;
    let response = client
        .get(format!(
            "{base_url}/api/v3/order?{query}&signature={signature}"
        ))
        .header("X-MBX-APIKEY", api_key)
        .send()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("order status request failed: {error}"),
        })?;
    decode_order_submission_json(response, "order status")
}

fn accepted_order_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    side: OrderSide,
    fallback_price: f64,
) -> Option<AcceptedOrder> {
    let accepted_quantity = net_quantity_from_binance_payload(payload, side);
    if accepted_quantity <= f64::EPSILON {
        return None;
    }

    let weighted_fill_price =
        payload
            .fills
            .iter()
            .fold((0.0, 0.0), |(notional_sum, qty_sum), fill| {
                (notional_sum + (fill.price * fill.qty), qty_sum + fill.qty)
            });
    let fill_price = if weighted_fill_price.1 > f64::EPSILON {
        weighted_fill_price.0 / weighted_fill_price.1
    } else if let Some(cumulative_quote_qty) = payload.cumulative_quote_qty {
        if cumulative_quote_qty > f64::EPSILON {
            cumulative_quote_qty / payload.executed_qty
        } else {
            fallback_price.max(0.0)
        }
    } else {
        fallback_price.max(0.0)
    };
    let (fee_asset, fee_amount, fee_normalized_usd) =
        fee_details_from_binance_payload(payload, fill_price);

    Some(AcceptedOrder {
        client_order_id: payload.client_order_id.clone(),
        side,
        order_type: OrderType::Market,
        price: fill_price,
        quantity: accepted_quantity,
        fee_asset,
        fee_amount,
        fee_normalized_usd,
    })
}

fn fee_details_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    fill_price: f64,
) -> (Option<String>, Option<f64>, Option<f64>) {
    let mut fee_asset = None;
    let mut fee_amount = 0.0;
    let mut mixed_assets = false;
    let mut fee_normalized_usd = 0.0;
    let mut has_normalized_fee = false;
    let quote_asset = usd_quote_asset_from_symbol(payload.symbol.as_str());
    let base_asset = quote_asset.and_then(|quote_asset| payload.symbol.strip_suffix(quote_asset));

    for fill in &payload.fills {
        let Some(commission_asset) = fill.commission_asset.as_ref() else {
            continue;
        };
        let Some(commission) = fill
            .commission
            .filter(|value| value.is_finite() && *value > 0.0)
        else {
            continue;
        };

        match fee_asset.as_deref() {
            None => fee_asset = Some(commission_asset.clone()),
            Some(existing_asset) if existing_asset == commission_asset => {}
            Some(_) => mixed_assets = true,
        }
        fee_amount += commission;

        let normalized_fee =
            if quote_asset.is_some_and(|asset| asset.eq_ignore_ascii_case(commission_asset)) {
                Some(commission)
            } else if base_asset.is_some_and(|asset| asset.eq_ignore_ascii_case(commission_asset))
                && fill_price.is_finite()
                && fill_price > f64::EPSILON
            {
                Some(commission * fill_price)
            } else {
                None
            };

        if let Some(normalized_fee) = normalized_fee {
            fee_normalized_usd += normalized_fee;
            has_normalized_fee = true;
        }
    }

    (
        if mixed_assets { None } else { fee_asset },
        if !mixed_assets && fee_amount > f64::EPSILON {
            Some(fee_amount)
        } else {
            None
        },
        if has_normalized_fee {
            Some(fee_normalized_usd)
        } else {
            None
        },
    )
}

fn usd_quote_asset_from_symbol(symbol: &str) -> Option<&'static str> {
    ["USDT", "USDC", "FDUSD", "BUSD", "USD"]
        .into_iter()
        .find(|quote_asset| symbol.ends_with(quote_asset))
}

fn net_quantity_from_binance_payload(
    payload: &BinanceSubmittedOrderPayload,
    side: OrderSide,
) -> f64 {
    if !matches!(side, OrderSide::Buy) {
        return payload.executed_qty.max(0.0);
    }

    let base_asset_commission = payload
        .fills
        .iter()
        .filter_map(|fill| {
            let commission_asset = fill.commission_asset.as_deref()?;
            let base_asset = payload.symbol.get(..commission_asset.len())?;
            if base_asset.eq_ignore_ascii_case(commission_asset) {
                fill.commission
            } else {
                None
            }
        })
        .sum::<f64>();

    (payload.executed_qty - base_asset_commission).max(0.0)
}

fn binance_order_status_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "FILLED" | "CANCELED" | "REJECTED" | "EXPIRED" | "EXPIRED_IN_MATCH"
    )
}

fn binance_order_side_label(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "BUY",
        OrderSide::Sell => "SELL",
    }
}

fn normalize_market_data_event(
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

#[allow(clippy::too_many_lines)]
async fn run_binance_preview_stream_worker(
    account: ConnectorAccount,
    mut command_rx: mpsc::UnboundedReceiver<ConnectorPreviewStreamCommand>,
    event_tx: mpsc::UnboundedSender<ConnectorPreviewStreamEvent>,
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

        let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
            state: PreviewStreamConnectionState::Connecting,
            detail: None,
        });

        match connect_async(BINANCE_MARKET_STREAM_WS_URL).await {
            Ok((mut socket, _)) => {
                reconnect_failures = 0;
                let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                    state: PreviewStreamConnectionState::Connected,
                    detail: Some(format!("account={}", account.account_id)),
                });

                if let Err(error) = send_binance_subscription_change(
                    &mut socket,
                    "SUBSCRIBE",
                    &desired,
                    &mut request_id,
                )
                .await
                {
                    let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                        state: PreviewStreamConnectionState::Disconnected,
                        detail: Some(error.to_string()),
                    });
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
                                            let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                                                state: PreviewStreamConnectionState::Disconnected,
                                                detail: Some(error.to_string()),
                                            });
                                            reconnect = true;
                                            desired = next;
                                            continue;
                                        }
                                        if let Err(error) = send_binance_subscription_change(&mut socket, "SUBSCRIBE", &to_subscribe, &mut request_id).await {
                                            let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
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
                                            let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
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
                                                let _ = event_tx.send(ConnectorPreviewStreamEvent::BarUpdate {
                                                    subscription,
                                                    update,
                                                });
                                            }
                                            Ok(None) => {}
                                            Err(error) => {
                                                let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
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
                                        let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                                            state: PreviewStreamConnectionState::Disconnected,
                                            detail: Some(detail),
                                        });
                                        reconnect = true;
                                    }
                                    Some(Ok(_)) => {}
                                    Some(Err(error)) => {
                                        let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                                            state: PreviewStreamConnectionState::Disconnected,
                                            detail: Some(format!("Binance preview stream error: {error}")),
                                        });
                                        reconnect = true;
                                    }
                                    None => {
                                        let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
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
                let _ = event_tx.send(ConnectorPreviewStreamEvent::ConnectionState {
                    state: PreviewStreamConnectionState::Disconnected,
                    detail: Some(format!("failed to connect Binance preview stream: {error}")),
                });
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

fn normalize_private_event(
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

fn sign_query(secret: &str, query: &str) -> Result<String, ConnectorError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|error| {
        ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("failed to prepare request signature: {error}"),
        }
    })?;
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn decode_json_response<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("{operation} request returned {status}: {body}"),
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

fn decode_order_submission_json<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        return Err(ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("{operation} request returned {status}: {body}"),
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Binance,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

fn extract_symbol_constraints(
    payload: BinanceExchangeInfoPayload,
    symbol: &str,
) -> Result<ConnectorSymbolConstraints, ConnectorError> {
    let symbol_payload = payload
        .symbols
        .into_iter()
        .find(|entry| entry.symbol == symbol)
        .ok_or_else(|| ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("exchangeInfo did not include symbol `{symbol}`"),
        })?;

    if !symbol_payload.status.eq_ignore_ascii_case("TRADING") {
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!(
                "symbol `{symbol}` is not trading (status={})",
                symbol_payload.status
            ),
        });
    }

    let mut quantity_step = None;
    let mut min_quantity = None;
    let mut min_notional_usd: Option<f64> = None;

    for filter in symbol_payload.filters {
        match filter.filter_type.as_str() {
            "LOT_SIZE" => {
                if let Some(parsed_step) = parse_positive_decimal(filter.step_size.as_deref()) {
                    quantity_step = Some(parsed_step);
                }
                if let Some(parsed_min_qty) = parse_positive_decimal(filter.min_qty.as_deref()) {
                    min_quantity = Some(parsed_min_qty);
                }
            }
            "MIN_NOTIONAL" | "NOTIONAL" => {
                if let Some(parsed_notional) =
                    parse_positive_decimal(filter.min_notional.as_deref())
                {
                    min_notional_usd = Some(
                        min_notional_usd
                            .map_or(parsed_notional, |current| current.max(parsed_notional)),
                    );
                }
            }
            _ => {}
        }
    }

    Ok(ConnectorSymbolConstraints {
        fractional_entry_supported: None,
        quantity_step,
        min_quantity,
        min_notional_usd,
        source: Some("binance_exchange_info".to_owned()),
    })
}

fn parse_positive_decimal(raw: Option<&str>) -> Option<f64> {
    raw.and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn normalize_orders(payload: Vec<BinanceOpenOrderPayload>) -> Vec<ConnectorOpenOrder> {
    payload
        .into_iter()
        .map(|order| ConnectorOpenOrder {
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            status: order.status,
            quantity: order.orig_qty,
        })
        .collect()
}

fn normalize_positions(payload: Vec<BinanceBalancePayload>) -> Vec<ConnectorPosition> {
    payload
        .into_iter()
        .filter_map(|balance| {
            let quantity = balance.free + balance.locked;
            if quantity.abs() <= f64::EPSILON || is_binance_cash_asset(balance.asset.as_str()) {
                None
            } else {
                Some(ConnectorPosition {
                    symbol: balance.asset,
                    quantity,
                })
            }
        })
        .collect()
}

fn is_binance_cash_asset(asset: &str) -> bool {
    matches!(asset, "USD" | "USDT" | "USDC" | "BUSD")
}

fn normalize_balances(payload: Vec<BinanceBalancePayload>) -> Vec<ConnectorPrivateBalance> {
    payload
        .into_iter()
        .map(|balance| ConnectorPrivateBalance {
            asset: balance.asset,
            free: balance.free,
            locked: balance.locked,
        })
        .collect()
}

#[cfg(test)]
fn parse_kline_row(row: &[serde_json::Value]) -> Result<OhlcvBar, ConnectorError> {
    parse_kline_row_with_close_time(row).map(|(bar, _)| bar)
}

fn parse_kline_row_with_close_time(
    row: &[serde_json::Value],
) -> Result<(OhlcvBar, i64), ConnectorError> {
    if row.len() < 6 {
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("kline row expected at least 6 fields, got {}", row.len()),
        });
    }

    let open_time_ms = value_to_i64(&row[0], "open_time_ms")?;
    let timestamp = chrono::DateTime::from_timestamp_millis(open_time_ms).ok_or_else(|| {
        ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("kline open time `{open_time_ms}` is not a valid timestamp"),
        }
    })?;

    let close_time_ms = if row.len() >= 7 {
        value_to_i64(&row[6], "close_time_ms")?
    } else {
        open_time_ms
    };

    Ok((
        OhlcvBar {
            timestamp,
            open: value_to_f64(&row[1], "open")?,
            high: value_to_f64(&row[2], "high")?,
            low: value_to_f64(&row[3], "low")?,
            close: value_to_f64(&row[4], "close")?,
            volume: value_to_f64(&row[5], "volume")?,
        },
        close_time_ms,
    ))
}

fn normalize_recent_binance_klines(
    rows: Vec<Vec<serde_json::Value>>,
    limit: usize,
    now_ms: i64,
) -> Result<Vec<OhlcvBar>, ConnectorError> {
    let mut bars = rows
        .into_iter()
        .filter_map(|row| match parse_kline_row_with_close_time(&row) {
            Ok((bar, close_time_ms)) if close_time_ms < now_ms => Some(Ok(bar)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if bars.len() > limit {
        bars.drain(0..bars.len() - limit);
    }

    Ok(bars)
}

fn latest_confirmed_binance_bar(
    rows: Vec<Vec<serde_json::Value>>,
    now_ms: i64,
) -> Result<Option<OhlcvBar>, ConnectorError> {
    Ok(normalize_recent_binance_klines(rows, 1, now_ms)?
        .into_iter()
        .next())
}

fn value_to_i64(value: &serde_json::Value, field: &str) -> Result<i64, ConnectorError> {
    match value {
        serde_json::Value::Number(number) => {
            number
                .as_i64()
                .ok_or_else(|| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not an i64-compatible number"),
                })
        }
        serde_json::Value::String(raw) => {
            raw.parse::<i64>()
                .map_err(|_| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not a valid integer: `{raw}`"),
                })
        }
        _ => Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("field `{field}` must be string or number"),
        }),
    }
}

fn value_to_f64(value: &serde_json::Value, field: &str) -> Result<f64, ConnectorError> {
    match value {
        serde_json::Value::Number(number) => {
            number
                .as_f64()
                .ok_or_else(|| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not an f64-compatible number"),
                })
        }
        serde_json::Value::String(raw) => {
            raw.parse::<f64>()
                .map_err(|_| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Binance,
                    detail: format!("field `{field}` is not a valid decimal: `{raw}`"),
                })
        }
        _ => Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Binance,
            detail: format!("field `{field}` must be string or number"),
        }),
    }
}

fn binance_interval_label(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1m",
        Timeframe::M5 => "5m",
        Timeframe::M15 => "15m",
        Timeframe::M30 => "30m",
        Timeframe::H1 => "1h",
        Timeframe::H4 => "4h",
        Timeframe::D1 => "1d",
    }
}

fn deserialize_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(raw) => raw
            .parse::<f64>()
            .map_err(|_| de::Error::custom(format!("expected numeric string, received `{raw}`"))),
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| de::Error::custom("expected f64-compatible number")),
        _ => Err(de::Error::custom("expected string or number")),
    }
}

fn deserialize_option_f64_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    let parsed = match value {
        serde_json::Value::String(raw) => raw
            .parse::<f64>()
            .map_err(|_| de::Error::custom(format!("expected numeric string, received `{raw}`")))?,
        serde_json::Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| de::Error::custom("expected f64-compatible number"))?,
        serde_json::Value::Null => return Ok(None),
        _ => return Err(de::Error::custom("expected string, number, or null")),
    };

    if parsed.abs() <= f64::EPSILON {
        return Ok(None);
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use openticker_core::TradeIntent;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn assert_f64_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
    }

    fn account(mode: ExecutionMode, use_demo_mode: bool) -> ConnectorAccount {
        ConnectorAccount {
            account_id: "binance-demo".to_owned(),
            kind: ConnectorKind::Binance,
            mode,
            use_demo_mode,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: false,
            reconciliation_base_url: None,
        }
    }

    fn remote_submission_account(base_url: String) -> ConnectorAccount {
        let mut configured = account(ExecutionMode::Paper, true);
        configured.reconciliation_remote_snapshot = true;
        configured.execution_remote_submission = true;
        configured.reconciliation_base_url = Some(base_url);
        configured.api_key_env = Some("PATH".to_owned());
        configured.api_secret_env = Some("PATH".to_owned());
        configured
    }

    #[test]
    fn resolves_demo_base_url_for_paper_or_demo_modes() {
        let paper = BinanceConnector::new(&account(ExecutionMode::Paper, true));
        assert_eq!(paper.reconciliation_base_url(), BINANCE_DEMO_BASE_URL);

        let live_demo = BinanceConnector::new(&account(ExecutionMode::Live, true));
        assert_eq!(live_demo.reconciliation_base_url(), BINANCE_DEMO_BASE_URL);

        let live = BinanceConnector::new(&account(ExecutionMode::Live, false));
        assert_eq!(live.reconciliation_base_url(), BINANCE_LIVE_BASE_URL);
    }

    #[test]
    fn signature_is_stable_for_identical_query() {
        let query = "recvWindow=5000&timestamp=1700000000000";
        let first = sign_query("secret", query).unwrap();
        let second = sign_query("secret", query).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn normalizes_open_orders_and_non_zero_balances() {
        let orders = serde_json::from_str::<Vec<BinanceOpenOrderPayload>>(
            r#"[
                {
                    "clientOrderId": "order-1",
                    "symbol": "BTCUSDT",
                    "status": "NEW",
                    "origQty": "0.01"
                }
            ]"#,
        )
        .unwrap();
        let account = serde_json::from_str::<BinanceAccountPayload>(
            r#"{
                "balances": [
                    {"asset": "BTC", "free": "0.5", "locked": "0.1"},
                    {"asset": "USDT", "free": "0", "locked": "0"}
                ]
            }"#,
        )
        .unwrap();

        let snapshot = ConnectorAccountSnapshot {
            open_orders: normalize_orders(orders),
            positions: normalize_positions(account.balances.clone()),
            balances: normalize_balances(account.balances),
        };

        assert_eq!(snapshot.open_orders.len(), 1);
        assert_eq!(snapshot.open_orders[0].symbol, "BTCUSDT");
        assert_f64_eq(snapshot.open_orders[0].quantity, 0.01);
        assert_eq!(snapshot.positions.len(), 1);
        assert_eq!(snapshot.positions[0].symbol, "BTC");
        assert_f64_eq(snapshot.positions[0].quantity, 0.6);
        assert_eq!(snapshot.balances.len(), 2);
        assert_eq!(snapshot.balances[0].asset, "BTC");
        assert_f64_eq(snapshot.balances[0].free, 0.5);
        assert_eq!(snapshot.balances[1].asset, "USDT");
    }

    #[test]
    fn excludes_cash_balances_from_binance_positions() {
        let account = serde_json::from_str::<BinanceAccountPayload>(
            r#"{
                "balances": [
                    {"asset": "BTC", "free": "0.01", "locked": "0"},
                    {"asset": "USDT", "free": "9997.48", "locked": "0"},
                    {"asset": "USDC", "free": "12.5", "locked": "0"}
                ]
            }"#,
        )
        .unwrap();

        let positions = normalize_positions(account.balances);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].symbol, "BTC");
        assert_f64_eq(positions[0].quantity, 0.01);
    }

    #[test]
    fn extracts_symbol_constraints_from_exchange_info() {
        let payload = serde_json::from_str::<BinanceExchangeInfoPayload>(
            r#"{
                "symbols": [
                    {
                        "symbol": "BTCUSDT",
                        "status": "TRADING",
                        "filters": [
                            {
                                "filterType": "LOT_SIZE",
                                "minQty": "0.00001000",
                                "stepSize": "0.00001000"
                            },
                            {
                                "filterType": "MIN_NOTIONAL",
                                "minNotional": "5.00000000"
                            }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();

        let constraints = extract_symbol_constraints(payload, "BTCUSDT").unwrap();
        assert_eq!(constraints.fractional_entry_supported, None);
        assert_f64_eq(constraints.quantity_step.unwrap(), 0.00001);
        assert_f64_eq(constraints.min_quantity.unwrap(), 0.00001);
        assert_f64_eq(constraints.min_notional_usd.unwrap(), 5.0);
        assert_eq!(constraints.source.as_deref(), Some("binance_exchange_info"));
    }

    #[test]
    fn formats_binance_quantity_without_excess_precision() {
        assert_eq!(
            format_binance_quantity(0.000_140_000_000_000_000_01),
            "0.00014"
        );
        assert_eq!(
            format_binance_quantity(2_499_999.999_990_000_4),
            "2499999.99999"
        );
    }

    #[test]
    fn rejects_symbol_constraints_when_symbol_is_not_trading() {
        let payload = serde_json::from_str::<BinanceExchangeInfoPayload>(
            r#"{
                "symbols": [
                    {
                        "symbol": "BTCUSDT",
                        "status": "BREAK",
                        "filters": []
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(matches!(
            extract_symbol_constraints(payload, "BTCUSDT"),
            Err(ConnectorError::RemoteSnapshot { .. })
        ));
    }

    #[test]
    fn maps_core_timeframes_to_binance_intervals() {
        assert_eq!(binance_interval_label(Timeframe::M1), "1m");
        assert_eq!(binance_interval_label(Timeframe::M5), "5m");
        assert_eq!(binance_interval_label(Timeframe::M15), "15m");
        assert_eq!(binance_interval_label(Timeframe::M30), "30m");
        assert_eq!(binance_interval_label(Timeframe::H1), "1h");
        assert_eq!(binance_interval_label(Timeframe::H4), "4h");
        assert_eq!(binance_interval_label(Timeframe::D1), "1d");
    }

    #[test]
    fn parses_binance_kline_row_into_bar() {
        let row = vec![
            serde_json::json!(1_704_067_200_000_i64),
            serde_json::json!("42000.1"),
            serde_json::json!("42100.0"),
            serde_json::json!("41950.2"),
            serde_json::json!("42080.4"),
            serde_json::json!("12.5"),
        ];

        let bar = parse_kline_row(&row).unwrap();
        assert_f64_eq(bar.open, 42_000.1);
        assert_f64_eq(bar.high, 42_100.0);
        assert_f64_eq(bar.low, 41_950.2);
        assert_f64_eq(bar.close, 42_080.4);
        assert_f64_eq(bar.volume, 12.5);
    }

    #[test]
    fn recent_binance_klines_are_oldest_first_and_confirmed_only() {
        let rows = vec![
            vec![
                serde_json::json!(1_704_067_200_000_i64),
                serde_json::json!("42000.1"),
                serde_json::json!("42100.0"),
                serde_json::json!("41950.2"),
                serde_json::json!("42080.4"),
                serde_json::json!("12.5"),
                serde_json::json!(1_704_067_259_999_i64),
            ],
            vec![
                serde_json::json!(1_704_067_260_000_i64),
                serde_json::json!("42080.4"),
                serde_json::json!("42150.0"),
                serde_json::json!("42000.0"),
                serde_json::json!("42120.0"),
                serde_json::json!("10.0"),
                serde_json::json!(1_704_067_319_999_i64),
            ],
            vec![
                serde_json::json!(1_704_067_320_000_i64),
                serde_json::json!("42120.0"),
                serde_json::json!("42200.0"),
                serde_json::json!("42050.0"),
                serde_json::json!("42180.0"),
                serde_json::json!("11.0"),
                serde_json::json!(1_704_067_379_999_i64),
            ],
        ];

        let normalized = normalize_recent_binance_klines(rows, 2, 1_704_067_350_000).unwrap();

        assert_eq!(normalized.len(), 2);
        assert_eq!(
            normalized[0].timestamp.timestamp_millis(),
            1_704_067_200_000
        );
        assert_eq!(
            normalized[1].timestamp.timestamp_millis(),
            1_704_067_260_000
        );
        assert_f64_eq(normalized[0].close, 42_080.4);
        assert_f64_eq(normalized[1].close, 42_120.0);
    }

    #[test]
    fn latest_binance_bar_ignores_open_kline() {
        let rows = vec![
            vec![
                serde_json::json!(1_704_067_260_000_i64),
                serde_json::json!("42080.4"),
                serde_json::json!("42150.0"),
                serde_json::json!("42000.0"),
                serde_json::json!("42120.0"),
                serde_json::json!("10.0"),
                serde_json::json!(1_704_067_319_999_i64),
            ],
            vec![
                serde_json::json!(1_704_067_320_000_i64),
                serde_json::json!("42120.0"),
                serde_json::json!("42200.0"),
                serde_json::json!("42050.0"),
                serde_json::json!("42180.0"),
                serde_json::json!("11.0"),
                serde_json::json!(1_704_067_379_999_i64),
            ],
        ];

        let latest = latest_confirmed_binance_bar(rows, 1_704_067_350_000)
            .unwrap()
            .expect("expected a confirmed kline");

        assert_eq!(latest.timestamp.timestamp_millis(), 1_704_067_260_000);
        assert_f64_eq(latest.close, 42_120.0);
    }

    #[test]
    fn normalizes_binance_kline_websocket_payloads() {
        let combined = r#"{
            "stream": "btcusdt@kline_1m",
            "data": {
                "e": "kline",
                "s": "BTCUSDT",
                "k": {
                    "t": 1704067200000,
                    "o": "42000.0",
                    "h": "42100.0",
                    "l": "41900.0",
                    "c": "42050.0",
                    "v": "15.5",
                    "x": false
                }
            }
        }"#;

        let preview = normalize_market_data_event(combined).unwrap().unwrap();
        assert_eq!(preview.symbol, "BTCUSDT");
        assert_eq!(preview.phase, SignalPhase::Preview);
        assert_f64_eq(preview.bar.close, 42_050.0);

        let raw = r#"{
            "e": "kline",
            "s": "BTCUSDT",
            "k": {
                "t": 1704067260000,
                "o": "42050.0",
                "h": "42200.0",
                "l": "42000.0",
                "c": "42180.0",
                "v": "20.0",
                "x": true
            }
        }"#;

        let confirmed = normalize_market_data_event(raw).unwrap().unwrap();
        assert_eq!(confirmed.phase, SignalPhase::Confirmed);
        assert_f64_eq(confirmed.bar.close, 42_180.0);
    }

    #[test]
    fn rejects_malformed_market_stream_payload() {
        assert!(matches!(
            normalize_market_data_event("not-json"),
            Err(ConnectorError::StreamDecode { .. })
        ));
    }

    #[test]
    fn normalizes_binance_private_order_and_account_payloads() {
        let order_payload = r#"{
            "stream": "btcusdt@executionReport",
            "data": {
                "e": "executionReport",
                "s": "BTCUSDT",
                "c": "cli-123",
                "S": "BUY",
                "X": "PARTIALLY_FILLED",
                "q": "0.50",
                "z": "0.20",
                "L": "42123.40"
            }
        }"#;

        let order_event = normalize_private_event(order_payload).unwrap().unwrap();
        let ConnectorPrivateStreamEvent::Order(order) = order_event else {
            panic!("expected normalized order event");
        };
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.client_order_id, "cli-123");
        assert_eq!(order.status, "PARTIALLY_FILLED");
        assert_eq!(order.side, "BUY");
        assert_f64_eq(order.order_quantity, 0.5);
        assert_f64_eq(order.cumulative_filled_quantity, 0.2);
        assert_eq!(order.last_fill_price, Some(42_123.4));

        let account_payload = r#"{
            "e": "outboundAccountPosition",
            "B": [
                {"a": "BTC", "f": "0.3", "l": "0.1"},
                {"a": "USDT", "f": "1000", "l": "5.25"}
            ]
        }"#;

        let account_event = normalize_private_event(account_payload).unwrap().unwrap();
        let ConnectorPrivateStreamEvent::Account(account) = account_event else {
            panic!("expected normalized account event");
        };
        assert_eq!(account.balances.len(), 2);
        assert_eq!(account.balances[0].asset, "BTC");
        assert_f64_eq(account.balances[0].free, 0.3);
        assert_f64_eq(account.balances[0].locked, 0.1);
        assert_eq!(account.balances[1].asset, "USDT");
        assert_f64_eq(account.balances[1].free, 1_000.0);
        assert_f64_eq(account.balances[1].locked, 5.25);
    }

    #[test]
    fn ignores_unhandled_private_stream_event_types() {
        let payload = r#"{
            "e": "listenKeyExpired"
        }"#;

        assert!(normalize_private_event(payload).unwrap().is_none());
    }

    #[test]
    fn rejects_malformed_private_stream_payload() {
        assert!(matches!(
            normalize_private_event("not-json"),
            Err(ConnectorError::StreamDecode { .. })
        ));
    }

    #[test]
    fn submits_remote_market_order_when_remote_snapshot_mode_enabled() {
        let (base_url, handle) = spawn_mock_binance_order_server();
        let connector = BinanceConnector::new(&remote_submission_account(base_url));
        let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let request = ExecutionRequest {
            instance_id: "btcusdt-instance".to_owned(),
            symbol: "BTCUSDT".to_owned(),
            timestamp,
            intent: TradeIntent::OpenLong,
            price: 42_050.0,
            quantity: 0.01,
        };

        let accepted = connector.submit_order(&request).unwrap();
        assert_eq!(accepted.side, OrderSide::Buy);
        assert_eq!(accepted.order_type, OrderType::Market);
        assert_f64_eq(accepted.quantity, 0.00999);
        assert_f64_eq(accepted.price, 42_050.0);
        assert_eq!(accepted.fee_asset.as_deref(), Some("BTC"));
        assert_f64_eq(accepted.fee_amount.unwrap(), 0.00001);
        assert_f64_eq(accepted.fee_normalized_usd.unwrap(), 0.4205);

        let request_lines = handle.join().unwrap();
        assert!(
            request_lines
                .iter()
                .any(|line| line.starts_with("POST /api/v3/order?"))
        );
    }

    #[test]
    fn keeps_binance_buy_quantity_when_fee_is_not_in_base_asset() {
        let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
            "symbol": "BTCUSDT",
            "clientOrderId": "binance-test-order-2",
            "status": "FILLED",
            "executedQty": "0.01000000",
            "cummulativeQuoteQty": "420.50000000",
            "fills": [
                {
                    "price": "42050.0",
                    "qty": "0.01",
                    "commission": "0.00002000",
                    "commissionAsset": "BNB"
                }
            ]
        }))
        .unwrap();

        let accepted =
            accepted_order_from_binance_payload(&payload, OrderSide::Buy, 42_050.0).unwrap();

        assert_f64_eq(accepted.quantity, 0.01);
        assert_f64_eq(accepted.price, 42_050.0);
        assert_eq!(accepted.fee_asset.as_deref(), Some("BNB"));
        assert_f64_eq(accepted.fee_amount.unwrap(), 0.00002);
        assert!(accepted.fee_normalized_usd.is_none());
    }

    #[test]
    fn keeps_binance_sell_quantity_when_fee_is_in_quote_asset() {
        let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
            "symbol": "ETHBTC",
            "clientOrderId": "binance-test-order-3",
            "status": "FILLED",
            "executedQty": "1.25000000",
            "cummulativeQuoteQty": "0.08000000",
            "fills": [
                {
                    "price": "0.064",
                    "qty": "1.25",
                    "commission": "0.00008000",
                    "commissionAsset": "BTC"
                }
            ]
        }))
        .unwrap();

        let accepted =
            accepted_order_from_binance_payload(&payload, OrderSide::Sell, 0.064).unwrap();

        assert_f64_eq(accepted.quantity, 1.25);
        assert_f64_eq(accepted.price, 0.064);
        assert_eq!(accepted.fee_asset.as_deref(), Some("BTC"));
        assert_f64_eq(accepted.fee_amount.unwrap(), 0.00008);
        assert!(accepted.fee_normalized_usd.is_none());
    }

    #[test]
    fn falls_back_to_executed_qty_when_binance_fee_details_are_missing() {
        let payload = serde_json::from_value::<BinanceSubmittedOrderPayload>(serde_json::json!({
            "symbol": "BTCUSDT",
            "clientOrderId": "binance-test-order-4",
            "status": "FILLED",
            "executedQty": "0.01000000",
            "cummulativeQuoteQty": "420.50000000",
            "fills": [
                {
                    "price": "42050.0",
                    "qty": "0.01"
                }
            ]
        }))
        .unwrap();

        let accepted =
            accepted_order_from_binance_payload(&payload, OrderSide::Buy, 42_050.0).unwrap();

        assert_f64_eq(accepted.quantity, 0.01);
        assert_f64_eq(accepted.price, 42_050.0);
        assert!(accepted.fee_asset.is_none());
        assert!(accepted.fee_amount.is_none());
        assert!(accepted.fee_normalized_usd.is_none());
    }

    fn spawn_mock_binance_order_server() -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request_with_body(&mut stream);
            let request_lines = request.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

            let response_body = r#"{
                "symbol": "BTCUSDT",
                "clientOrderId": "binance-test-order-1",
                "status": "FILLED",
                "executedQty": "0.01000000",
                "cummulativeQuoteQty": "420.50000000",
                "fills": [
                    {
                        "price": "42050.0",
                        "qty": "0.01",
                        "commission": "0.00001000",
                        "commissionAsset": "BTC"
                    }
                ]
            }"#;
            write_http_response(&mut stream, "HTTP/1.1 200 OK", response_body);
            request_lines
        });

        (format!("http://{address}"), handle)
    }

    fn read_http_request_with_body(stream: &mut TcpStream) -> String {
        let mut buffer = [0_u8; 2048];
        let mut data = Vec::new();
        let mut header_len = None;
        let mut content_len = 0_usize;

        loop {
            let count = stream.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..count]);

            if header_len.is_none()
                && let Some(index) = data.windows(4).position(|window| window == b"\r\n\r\n")
            {
                let end = index + 4;
                header_len = Some(end);
                let headers = String::from_utf8_lossy(&data[..end]);
                content_len = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
            }

            if let Some(header_len) = header_len
                && data.len() >= header_len + content_len
            {
                break;
            }
        }

        String::from_utf8_lossy(&data).into_owned()
    }

    fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) {
        let response = format!(
            "{status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
    }
}
