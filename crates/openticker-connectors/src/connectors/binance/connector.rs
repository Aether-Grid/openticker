use super::http::{decode_json_response, sign_query};
use super::klines::{
    binance_interval_label, latest_confirmed_binance_bar, normalize_recent_binance_klines,
    parse_kline_row_with_close_time,
};
use super::orders::{
    accepted_order_from_binance_payload, binance_order_side_label,
    binance_order_status_is_terminal, fetch_binance_order_status, format_binance_quantity,
    submit_binance_market_order,
};
use super::snapshot::{
    BinanceAccountPayload, BinanceExchangeInfoPayload, BinanceOpenOrderPayload,
    extract_symbol_constraints, normalize_balances, normalize_orders, normalize_positions,
};
use super::stream::{
    normalize_market_data_event, normalize_private_event, run_binance_preview_stream_worker,
};
use crate::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot, ConnectorError,
    ConnectorExecution, ConnectorHealth, ConnectorKind, ConnectorMarketData, ConnectorMarketStream,
    ConnectorPreviewStreamSession, ConnectorPrivateStream, ConnectorPrivateStreamEvent,
    ConnectorReconcile, ConnectorResiliencePolicy, ConnectorResilienceState,
    ConnectorRuntimeControl, ConnectorStatus, ConnectorSymbolConstraints,
    ConnectorSymbolConstraintsLookup, PREVIEW_STREAM_COMMAND_CAPACITY,
    PREVIEW_STREAM_EVENT_CAPACITY, StubConnector, default_blocking_http_client, descriptor_for,
    deterministic_remote_client_order_id, resolve_secret_env_value, run_in_blocking_thread,
    sanitize_symbol_for_error, unix_now_ms,
};
use openticker_core::{ExecutionMode, OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, ExecutionRouter, PaperExecutionRouter,
};
use std::time::Duration;
use tokio::sync::mpsc;

pub(super) const BINANCE_LIVE_BASE_URL: &str = "https://api.binance.com";
pub(super) const BINANCE_DEMO_BASE_URL: &str = "https://demo-api.binance.com";
const BINANCE_ORDER_STATUS_POLL_ATTEMPTS: u8 = 20;
const BINANCE_ORDER_STATUS_POLL_INTERVAL_MS: u64 = 250;
/// Upper bound on the order-status polling interval. The interval starts at
/// [`BINANCE_ORDER_STATUS_POLL_INTERVAL_MS`] and doubles each attempt (capped
/// here) so a slow-to-fill order backs off instead of polling at a fixed fast
/// rate. A `429`/`418` response short-circuits the loop entirely because
/// `fetch_binance_order_status` surfaces [`ConnectorError::RateLimited`].
const BINANCE_ORDER_STATUS_POLL_MAX_INTERVAL_MS: u64 = 4_000;

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

    pub(super) fn reconciliation_base_url(&self) -> String {
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
                        "latest kline response did not include confirmed data for `{}`",
                        sanitize_symbol_for_error(&symbol)
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
                        // A bar is confirmed only when its close time is
                        // strictly before `now`; a kline closing exactly at
                        // `now_ms` is still forming. This matches the strict
                        // `<` comparison in `normalize_recent_binance_klines`
                        // so both code paths agree on confirmation.
                        Ok((bar, close_time_ms))
                            if close_time_ms < now_ms
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
            let quantity = format_binance_quantity(request_quantity)?;
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

                // Exponential backoff: double the base interval each attempt up
                // to the cap so a slow-to-fill order eases off the API instead
                // of polling at a fixed 250ms. A rate-limit response below
                // breaks the loop via `?` (RateLimited).
                let backoff_ms = BINANCE_ORDER_STATUS_POLL_INTERVAL_MS
                    .saturating_mul(1_u64 << u32::from(attempt).min(16))
                    .min(BINANCE_ORDER_STATUS_POLL_MAX_INTERVAL_MS);
                std::thread::sleep(Duration::from_millis(backoff_ms));
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
        let (command_tx, command_rx) = mpsc::channel(PREVIEW_STREAM_COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(PREVIEW_STREAM_EVENT_CAPACITY);
        let account = self.account.clone();
        let worker = tokio::spawn(run_binance_preview_stream_worker(
            account, command_rx, event_tx,
        ));
        Ok(Some(ConnectorPreviewStreamSession::new(
            command_tx, event_rx, worker,
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
