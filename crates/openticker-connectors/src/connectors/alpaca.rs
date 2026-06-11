use crate::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot, ConnectorError,
    ConnectorExecution, ConnectorHealth, ConnectorKind, ConnectorMarketData, ConnectorMarketStream,
    ConnectorOpenOrder, ConnectorPosition, ConnectorPreviewStreamSession, ConnectorPrivateBalance,
    ConnectorPrivateStream, ConnectorPrivateStreamEvent, ConnectorReconcile,
    ConnectorResiliencePolicy, ConnectorResilienceState, ConnectorRuntimeControl, ConnectorStatus,
    ConnectorSymbolConstraints, ConnectorSymbolConstraintsLookup, StubConnector,
    default_blocking_http_client, descriptor_for, deterministic_remote_client_order_id,
    format_decimal_quantity, rate_limit_error, resolve_secret_env_value, retry_after_header,
    run_in_blocking_thread, sanitize_symbol_for_error,
};
use openticker_core::{ExecutionMode, OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, ExecutionRouter, OrderSide, OrderType, PaperExecutionRouter,
};
use reqwest::blocking::Response;
use serde::Deserialize;
use serde::de::{self, Deserializer};
use std::collections::HashMap;
use std::time::Duration;

const ALPACA_PAPER_BASE_URL: &str = "https://paper-api.alpaca.markets";
const ALPACA_LIVE_BASE_URL: &str = "https://api.alpaca.markets";
const ALPACA_DATA_BASE_URL: &str = "https://data.alpaca.markets";
const ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS: i64 = 30;
const ALPACA_RECENT_BARS_LOOKBACK_MAX_DAYS: i64 = 730;
const ALPACA_RECENT_BARS_LOOKBACK_SLACK: i64 = 6;
const ALPACA_ORDER_STATUS_POLL_ATTEMPTS: u8 = 20;
const ALPACA_ORDER_STATUS_POLL_INTERVAL_MS: u64 = 250;
/// Upper bound on the order-status polling interval. The interval starts at
/// [`ALPACA_ORDER_STATUS_POLL_INTERVAL_MS`] and doubles each attempt (capped
/// here) so a slow-to-fill order backs off instead of polling at a fixed fast
/// rate. A `429`/`418` response short-circuits the loop entirely because the
/// status fetch decodes through `decode_order_submission_json`, which surfaces
/// [`ConnectorError::RateLimited`].
const ALPACA_ORDER_STATUS_POLL_MAX_INTERVAL_MS: u64 = 4_000;

#[derive(Debug, Clone)]
pub struct AlpacaConnector {
    inner: StubConnector,
    account: ConnectorAccount,
    resilience: ConnectorResilienceState,
}

impl AlpacaConnector {
    #[must_use]
    pub fn new(account: &ConnectorAccount) -> Self {
        Self {
            inner: StubConnector::new(ConnectorKind::Alpaca, account.mode, account.use_demo_mode),
            account: account.clone(),
            resilience: ConnectorResilienceState::default(),
        }
    }

    #[must_use]
    pub fn resilience_policy() -> ConnectorResiliencePolicy {
        descriptor_for(ConnectorKind::Alpaca).resilience
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
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let base_url = self.reconciliation_base_url();

        run_in_blocking_thread(ConnectorKind::Alpaca, "alpaca-remote-snapshot", move || {
            let orders_response = client
                .get(format!("{base_url}/v2/orders"))
                .query(&[
                    ("status", "open"),
                    ("direction", "desc"),
                    ("nested", "false"),
                    ("limit", "500"),
                ])
                .header("APCA-API-KEY-ID", api_key.as_str())
                .header("APCA-API-SECRET-KEY", api_secret.as_str())
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Alpaca,
                    detail: format!("open orders request failed: {error}"),
                })?;
            let orders_payload: Vec<AlpacaOrderPayload> =
                decode_json_response(orders_response, "open orders")?;

            let positions_response = client
                .get(format!("{base_url}/v2/positions"))
                .header("APCA-API-KEY-ID", api_key.as_str())
                .header("APCA-API-SECRET-KEY", api_secret.as_str())
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Alpaca,
                    detail: format!("positions request failed: {error}"),
                })?;
            let positions_payload: Vec<AlpacaPositionPayload> =
                decode_json_response(positions_response, "positions")?;

            let account_response = client
                .get(format!("{base_url}/v2/account"))
                .header("APCA-API-KEY-ID", api_key.as_str())
                .header("APCA-API-SECRET-KEY", api_secret.as_str())
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Alpaca,
                    detail: format!("account request failed: {error}"),
                })?;
            let account_payload: AlpacaAccountPayload =
                decode_json_response(account_response, "account")?;

            Ok(ConnectorAccountSnapshot {
                open_orders: normalize_orders(orders_payload),
                positions: normalize_positions(positions_payload),
                balances: normalize_account_balances(&account_payload),
            })
        })
    }

    fn reconciliation_base_url(&self) -> String {
        self.account
            .reconciliation_base_url
            .clone()
            .unwrap_or_else(|| {
                match self.account.mode {
                    ExecutionMode::Paper => ALPACA_PAPER_BASE_URL,
                    ExecutionMode::Live => ALPACA_LIVE_BASE_URL,
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
        let api_key = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let symbol = symbol.to_owned();

        run_in_blocking_thread(ConnectorKind::Alpaca, "alpaca-latest-bar", move || {
            let now = chrono::Utc::now();
            let lookback_start = alpaca_recent_bars_lookback_start(now, timeframe, 1);
            let bars_response = client
                .get(format!("{ALPACA_DATA_BASE_URL}/v2/stocks/{symbol}/bars"))
                .query(&[
                    ("timeframe", alpaca_timeframe_label(timeframe)),
                    ("feed", "iex"),
                    ("adjustment", "raw"),
                    ("limit", "2"),
                    ("sort", "desc"),
                ])
                .query(&[("start", lookback_start.as_str())])
                .header("APCA-API-KEY-ID", api_key.as_str())
                .header("APCA-API-SECRET-KEY", api_secret.as_str())
                .send()
                .map_err(|error| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Alpaca,
                    detail: format!("latest bar request failed: {error}"),
                })?;
            let payload: AlpacaBarsPayload = decode_json_response(bars_response, "latest bar")?;

            latest_confirmed_alpaca_bar(payload.bars.unwrap_or_default(), timeframe, now)
                .ok_or_else(|| ConnectorError::RemoteSnapshot {
                    kind: ConnectorKind::Alpaca,
                    detail: format!(
                        "latest bar response did not include confirmed bar data for `{}`",
                        sanitize_symbol_for_error(&symbol)
                    ),
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

        let api_key = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let symbol = symbol.to_owned();
        let requested = limit.saturating_add(1).min(10_000);

        run_in_blocking_thread(ConnectorKind::Alpaca, "alpaca-recent-bars", move || {
            let now = chrono::Utc::now();
            let lookback_start = alpaca_recent_bars_lookback_start(now, timeframe, limit);
            let lookback_end = now.to_rfc3339();
            let requested_string = requested.to_string();
            let mut collected = Vec::new();
            let mut next_page_token = None;

            loop {
                let mut request = client
                    .get(format!("{ALPACA_DATA_BASE_URL}/v2/stocks/bars"))
                    .query(&[
                        ("symbols", symbol.as_str()),
                        ("timeframe", alpaca_timeframe_label(timeframe)),
                        ("feed", "iex"),
                        ("adjustment", "raw"),
                        ("limit", requested_string.as_str()),
                        ("sort", "desc"),
                        ("start", lookback_start.as_str()),
                        ("end", lookback_end.as_str()),
                    ])
                    .header("APCA-API-KEY-ID", api_key.as_str())
                    .header("APCA-API-SECRET-KEY", api_secret.as_str());

                if let Some(page_token) = next_page_token.as_deref() {
                    request = request.query(&[("page_token", page_token)]);
                }

                let bars_response =
                    request
                        .send()
                        .map_err(|error| ConnectorError::RemoteSnapshot {
                            kind: ConnectorKind::Alpaca,
                            detail: format!("recent bars request failed: {error}"),
                        })?;
                let payload: AlpacaHistoricalBarsPayload =
                    decode_json_response(bars_response, "recent bars")?;
                let page = historical_alpaca_bars_for_symbol(payload.bars, &symbol);
                collected.extend(page);

                if collected.len() >= requested {
                    break;
                }

                next_page_token = payload.next_page_token;
                if next_page_token.is_none() {
                    break;
                }
            }

            Ok(normalize_recent_alpaca_bars(
                collected, timeframe, now, limit,
            ))
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

        let api_key = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let symbol = symbol.to_owned();
        let requested = limit.max(1);

        run_in_blocking_thread(
            ConnectorKind::Alpaca,
            "alpaca-confirmed-bars-range",
            move || {
                let timeframe_duration =
                    chrono::Duration::from_std(timeframe.duration()).map_err(|error| {
                        ConnectorError::RemoteSnapshot {
                            kind: ConnectorKind::Alpaca,
                            detail: format!("unsupported timeframe duration: {error}"),
                        }
                    })?;
                let request_start = start_after.map_or_else(
                    || {
                        alpaca_recent_bars_lookback_start(
                            end_at + timeframe_duration,
                            timeframe,
                            requested,
                        )
                    },
                    |timestamp| (timestamp + chrono::Duration::milliseconds(1)).to_rfc3339(),
                );
                let request_end = (end_at + timeframe_duration).to_rfc3339();
                let request_limit = requested.to_string();

                let bars_response = client
                    .get(format!("{ALPACA_DATA_BASE_URL}/v2/stocks/bars"))
                    .query(&[
                        ("symbols", symbol.as_str()),
                        ("timeframe", alpaca_timeframe_label(timeframe)),
                        ("feed", "iex"),
                        ("adjustment", "raw"),
                        ("limit", request_limit.as_str()),
                        ("sort", "asc"),
                        ("start", request_start.as_str()),
                        ("end", request_end.as_str()),
                    ])
                    .header("APCA-API-KEY-ID", api_key.as_str())
                    .header("APCA-API-SECRET-KEY", api_secret.as_str())
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Alpaca,
                        detail: format!("confirmed range request failed: {error}"),
                    })?;
                let payload: AlpacaHistoricalBarsPayload =
                    decode_json_response(bars_response, "confirmed range")?;
                let bars = historical_alpaca_bars_for_symbol(payload.bars, &symbol);
                let normalized = normalize_confirmed_alpaca_range_bars(
                    bars,
                    timeframe,
                    end_at,
                    start_after,
                    requested,
                );

                Ok(ConfirmedBarPage {
                    exhausted: normalized.last().is_some_and(|bar| bar.timestamp >= end_at)
                        || normalized.len() < requested,
                    bars: normalized,
                })
            },
        )
    }

    fn fetch_remote_symbol_constraints(
        &self,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        let api_key = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let base_url = self.reconciliation_base_url();
        let symbol = symbol.to_owned();

        run_in_blocking_thread(
            ConnectorKind::Alpaca,
            "alpaca-symbol-constraints",
            move || {
                let response = client
                    .get(format!("{base_url}/v2/assets/{symbol}"))
                    .header("APCA-API-KEY-ID", api_key.as_str())
                    .header("APCA-API-SECRET-KEY", api_secret.as_str())
                    .send()
                    .map_err(|error| ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Alpaca,
                        detail: format!("asset metadata request failed: {error}"),
                    })?;
                let asset: AlpacaAssetPayload = decode_json_response(response, "asset metadata")?;
                if !asset.tradable {
                    return Err(ConnectorError::RemoteSnapshot {
                        kind: ConnectorKind::Alpaca,
                        detail: format!(
                            "asset `{}` is not tradable",
                            sanitize_symbol_for_error(&symbol)
                        ),
                    });
                }

                Ok(symbol_constraints_from_asset(&asset))
            },
        )
    }

    fn submit_remote_order(
        &self,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, ConnectorError> {
        let accepted_stub = PaperExecutionRouter.submit(request).map_err(|error| {
            ConnectorError::OrderSubmission {
                kind: ConnectorKind::Alpaca,
                detail: error.to_string(),
            }
        })?;
        let api_key = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_key_env",
            self.account.api_key_env.as_deref(),
        )?;
        let api_secret = resolve_secret_env_value(
            ConnectorKind::Alpaca,
            "api_secret_env",
            self.account.api_secret_env.as_deref(),
        )?;
        let client = default_blocking_http_client(ConnectorKind::Alpaca)?;
        let base_url = self.reconciliation_base_url();
        let symbol = request.symbol.clone();
        let request_price = request.price;
        let request_quantity = request.quantity;
        let side = accepted_stub.side;
        let client_order_id = deterministic_remote_client_order_id(ConnectorKind::Alpaca, request);

        run_in_blocking_thread(ConnectorKind::Alpaca, "alpaca-submit-order", move || {
            let payload = serde_json::json!({
                "symbol": symbol,
                "qty": format_decimal_quantity(request_quantity),
                "side": alpaca_order_side_label(side),
                "type": "market",
                "time_in_force": "day",
                "client_order_id": client_order_id,
            });

            let submit_response = client
                .post(format!("{base_url}/v2/orders"))
                .header("APCA-API-KEY-ID", api_key.as_str())
                .header("APCA-API-SECRET-KEY", api_secret.as_str())
                .json(&payload)
                .send()
                .map_err(|error| ConnectorError::OrderSubmission {
                    kind: ConnectorKind::Alpaca,
                    detail: format!("order submission request failed: {error}"),
                })?;
            let mut order_payload: AlpacaSubmittedOrderPayload =
                decode_order_submission_json(submit_response, "submit order")?;

            for attempt in 0..=ALPACA_ORDER_STATUS_POLL_ATTEMPTS {
                if alpaca_order_status_is_terminal(order_payload.status.as_str()) {
                    if let Some(accepted) =
                        accepted_order_from_alpaca_payload(&order_payload, side, request_price)
                    {
                        return Ok(accepted);
                    }
                    return Err(ConnectorError::OrderSubmission {
                        kind: ConnectorKind::Alpaca,
                        detail: format!(
                            "order `{}` reached terminal status `{}` without any fill quantity",
                            order_payload.client_order_id, order_payload.status
                        ),
                    });
                }

                if attempt == ALPACA_ORDER_STATUS_POLL_ATTEMPTS {
                    break;
                }

                // Exponential backoff: double the base interval each attempt up
                // to the cap so a slow-to-fill order eases off the API instead
                // of polling at a fixed 250ms. A rate-limit response below
                // breaks the loop via `?` (RateLimited).
                let backoff_ms = ALPACA_ORDER_STATUS_POLL_INTERVAL_MS
                    .saturating_mul(1_u64 << u32::from(attempt).min(16))
                    .min(ALPACA_ORDER_STATUS_POLL_MAX_INTERVAL_MS);
                std::thread::sleep(Duration::from_millis(backoff_ms));
                let status_response = client
                    .get(format!("{base_url}/v2/orders/{}", order_payload.id))
                    .header("APCA-API-KEY-ID", api_key.as_str())
                    .header("APCA-API-SECRET-KEY", api_secret.as_str())
                    .send()
                    .map_err(|error| ConnectorError::OrderSubmission {
                        kind: ConnectorKind::Alpaca,
                        detail: format!("order status request failed: {error}"),
                    })?;
                order_payload = decode_order_submission_json(status_response, "order status")?;
            }

            Err(ConnectorError::OrderSubmission {
                kind: ConnectorKind::Alpaca,
                detail: format!(
                    "order `{}` did not reach terminal status within {} polling attempts (latest status `{}`)",
                    order_payload.client_order_id,
                    ALPACA_ORDER_STATUS_POLL_ATTEMPTS,
                    order_payload.status
                ),
            })
        })
    }
}

impl ConnectorHealth for AlpacaConnector {
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

impl ConnectorReconcile for AlpacaConnector {
    fn fetch_account_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        if !self.account.reconciliation_remote_snapshot {
            return self.inner.fetch_account_snapshot();
        }
        self.fetch_remote_snapshot()
    }
}

impl ConnectorMarketData for AlpacaConnector {
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

impl ConnectorExecution for AlpacaConnector {
    fn submit_order(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ConnectorError> {
        if !self.account.execution_remote_submission {
            return self.inner.submit_order(request);
        }
        self.submit_remote_order(request)
    }
}

impl ConnectorSymbolConstraintsLookup for AlpacaConnector {
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

impl ConnectorMarketStream for AlpacaConnector {
    fn normalize_market_data_event(
        &self,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
        self.inner.normalize_market_data_event(payload)
    }

    fn start_preview_stream_session(
        &self,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError> {
        Ok(None)
    }
}

impl ConnectorPrivateStream for AlpacaConnector {
    fn normalize_private_stream_event(
        &self,
        payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
        self.inner.normalize_private_stream_event(payload)
    }
}

impl ConnectorRuntimeControl for AlpacaConnector {
    fn note_disconnect(&mut self, now_ms: i64) {
        AlpacaConnector::note_disconnect(self, now_ms);
    }

    fn note_reconnect_success(&mut self) {
        AlpacaConnector::note_reconnect_success(self);
    }

    fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64) {
        AlpacaConnector::note_rate_limit(self, now_ms, throttle_window_ms);
    }
}

#[derive(Debug, Deserialize)]
struct AlpacaOrderPayload {
    client_order_id: String,
    symbol: String,
    status: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    qty: f64,
}

#[derive(Debug, Deserialize)]
struct AlpacaPositionPayload {
    symbol: String,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    qty: f64,
}

#[derive(Debug, Deserialize)]
struct AlpacaAccountPayload {
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    cash: f64,
    #[serde(deserialize_with = "deserialize_f64_from_string_or_number")]
    equity: f64,
}

#[derive(Debug, Deserialize)]
struct AlpacaAssetPayload {
    tradable: bool,
    #[serde(default)]
    fractionable: bool,
}

fn symbol_constraints_from_asset(asset: &AlpacaAssetPayload) -> ConnectorSymbolConstraints {
    let (quantity_step, min_quantity) = if asset.fractionable {
        (None, None)
    } else {
        (Some(1.0), Some(1.0))
    };

    ConnectorSymbolConstraints {
        fractional_entry_supported: Some(asset.fractionable),
        quantity_step,
        min_quantity,
        min_notional_usd: None,
        source: Some(format!(
            "alpaca_asset_metadata(fractionable={})",
            asset.fractionable
        )),
    }
}

#[derive(Debug, Deserialize)]
struct AlpacaBarsPayload {
    #[serde(default)]
    bars: Option<Vec<AlpacaBarPayload>>,
}

#[derive(Debug, Deserialize)]
struct AlpacaHistoricalBarsPayload {
    #[serde(default)]
    bars: HashMap<String, Vec<AlpacaBarPayload>>,
    #[serde(default)]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlpacaBarPayload {
    #[serde(rename = "t")]
    timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "o")]
    open: f64,
    #[serde(rename = "h")]
    high: f64,
    #[serde(rename = "l")]
    low: f64,
    #[serde(rename = "c")]
    close: f64,
    #[serde(rename = "v")]
    volume: f64,
}

#[derive(Debug, Deserialize)]
struct AlpacaSubmittedOrderPayload {
    id: String,
    client_order_id: String,
    status: String,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    filled_qty: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_f64_from_string_or_number"
    )]
    filled_avg_price: Option<f64>,
}

fn alpaca_timeframe_label(timeframe: Timeframe) -> &'static str {
    match timeframe {
        Timeframe::M1 => "1Min",
        Timeframe::M5 => "5Min",
        Timeframe::M15 => "15Min",
        Timeframe::M30 => "30Min",
        Timeframe::H1 => "1Hour",
        Timeframe::H4 => "4Hour",
        Timeframe::D1 => "1Day",
    }
}

fn alpaca_recent_bars_lookback_start(
    now: chrono::DateTime<chrono::Utc>,
    timeframe: Timeframe,
    limit: usize,
) -> String {
    let requested_bars = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
    // Guard against a zero or sub-second timeframe duration: without a floor a
    // tiny duration collapses `lookback_seconds` toward zero and the lookback
    // window would degenerate. A minimum of one second per bar keeps the
    // computed window monotonic in `limit` before the day clamp applies.
    let seconds_per_bar = i64::try_from(timeframe.duration().as_secs())
        .unwrap_or(i64::MAX)
        .max(1);
    let lookback_seconds = seconds_per_bar
        .saturating_mul(requested_bars)
        .saturating_mul(ALPACA_RECENT_BARS_LOOKBACK_SLACK);
    let lookback_days = (lookback_seconds / 86_400).clamp(
        ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS,
        ALPACA_RECENT_BARS_LOOKBACK_MAX_DAYS,
    );

    (now - chrono::Duration::days(lookback_days)).to_rfc3339()
}

fn historical_alpaca_bars_for_symbol(
    mut bars: HashMap<String, Vec<AlpacaBarPayload>>,
    symbol: &str,
) -> Vec<AlpacaBarPayload> {
    bars.remove(symbol).unwrap_or_default()
}

fn alpaca_bar_is_confirmed(
    timestamp: chrono::DateTime<chrono::Utc>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    match chrono::Duration::from_std(timeframe.duration()) {
        Ok(duration) => timestamp + duration <= now,
        Err(_) => false,
    }
}

fn normalize_recent_alpaca_bars(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
    limit: usize,
) -> Vec<OhlcvBar> {
    let mut normalized = bars
        .into_iter()
        .map(|bar| OhlcvBar {
            timestamp: bar.timestamp,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .filter(|bar| alpaca_bar_is_confirmed(bar.timestamp, timeframe, now))
        .take(limit)
        .collect::<Vec<_>>();
    normalized.reverse();
    normalized
}

fn normalize_confirmed_alpaca_range_bars(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    end_at: chrono::DateTime<chrono::Utc>,
    start_after: Option<chrono::DateTime<chrono::Utc>>,
    limit: usize,
) -> Vec<OhlcvBar> {
    let now = chrono::Utc::now();
    bars.into_iter()
        .map(|bar| OhlcvBar {
            timestamp: bar.timestamp,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .filter(|bar| {
            alpaca_bar_is_confirmed(bar.timestamp, timeframe, now)
                && bar.timestamp <= end_at
                && start_after.is_none_or(|start| bar.timestamp > start)
        })
        .take(limit)
        .collect()
}

fn latest_confirmed_alpaca_bar(
    bars: Vec<AlpacaBarPayload>,
    timeframe: Timeframe,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<OhlcvBar> {
    normalize_recent_alpaca_bars(bars, timeframe, now, 1)
        .into_iter()
        .next()
}

fn decode_json_response<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_header(&response);
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        let detail = format!("{operation} request returned {status}: {body}");
        if let Some(error) = rate_limit_error(
            ConnectorKind::Alpaca,
            status,
            retry_after.as_deref(),
            detail.clone(),
        ) {
            return Err(error);
        }
        return Err(ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Alpaca,
            detail,
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::RemoteSnapshot {
            kind: ConnectorKind::Alpaca,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

fn decode_order_submission_json<T: for<'de> Deserialize<'de>>(
    response: Response,
    operation: &str,
) -> Result<T, ConnectorError> {
    let status = response.status();
    if !status.is_success() {
        let retry_after = retry_after_header(&response);
        let body = response
            .text()
            .unwrap_or_else(|_| "<response body unavailable>".to_owned());
        let detail = format!("{operation} request returned {status}: {body}");
        if let Some(error) = rate_limit_error(
            ConnectorKind::Alpaca,
            status,
            retry_after.as_deref(),
            detail.clone(),
        ) {
            return Err(error);
        }
        return Err(ConnectorError::OrderSubmission {
            kind: ConnectorKind::Alpaca,
            detail,
        });
    }

    response
        .json::<T>()
        .map_err(|error| ConnectorError::OrderSubmission {
            kind: ConnectorKind::Alpaca,
            detail: format!("failed to decode {operation} response: {error}"),
        })
}

fn accepted_order_from_alpaca_payload(
    payload: &AlpacaSubmittedOrderPayload,
    side: OrderSide,
    fallback_price: f64,
) -> Option<AcceptedOrder> {
    let quantity = payload.filled_qty.unwrap_or(0.0);
    if quantity <= f64::EPSILON {
        return None;
    }

    let price = payload
        .filled_avg_price
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback_price.max(0.0));

    Some(AcceptedOrder {
        client_order_id: payload.client_order_id.clone(),
        side,
        order_type: OrderType::Market,
        price,
        quantity,
        fee_asset: None,
        fee_amount: None,
        fee_normalized_usd: None,
    })
}

fn alpaca_order_side_label(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

fn alpaca_order_status_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "filled"
            | "canceled"
            | "expired"
            | "rejected"
            | "suspended"
            | "stopped"
            | "done_for_day"
            | "calculated"
            | "replaced"
    )
}

fn normalize_orders(payload: Vec<AlpacaOrderPayload>) -> Vec<ConnectorOpenOrder> {
    payload
        .into_iter()
        .map(|order| ConnectorOpenOrder {
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            status: order.status,
            quantity: order.qty,
        })
        .collect()
}

fn normalize_positions(payload: Vec<AlpacaPositionPayload>) -> Vec<ConnectorPosition> {
    payload
        .into_iter()
        .map(|position| ConnectorPosition {
            symbol: position.symbol,
            quantity: position.qty,
        })
        .collect()
}

fn normalize_account_balances(payload: &AlpacaAccountPayload) -> Vec<ConnectorPrivateBalance> {
    vec![
        ConnectorPrivateBalance {
            asset: "CASH".to_owned(),
            free: payload.cash,
            locked: 0.0,
        },
        ConnectorPrivateBalance {
            asset: "EQUITY".to_owned(),
            free: payload.equity,
            locked: 0.0,
        },
    ]
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

    if !parsed.is_finite() || parsed <= f64::EPSILON {
        return Ok(None);
    }
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use openticker_core::ExecutionMode;
    use openticker_core::TradeIntent;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn assert_f64_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
    }

    fn account(mode: ExecutionMode) -> ConnectorAccount {
        ConnectorAccount {
            account_id: "alpaca-paper".to_owned(),
            kind: ConnectorKind::Alpaca,
            mode,
            use_demo_mode: false,
            api_key_env: Some("PATH".to_owned()),
            api_secret_env: Some("PATH".to_owned()),
            passphrase_env: None,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: false,
            reconciliation_base_url: None,
        }
    }

    fn remote_submission_account(base_url: String) -> ConnectorAccount {
        let mut configured = account(ExecutionMode::Paper);
        configured.reconciliation_remote_snapshot = true;
        configured.execution_remote_submission = true;
        configured.reconciliation_base_url = Some(base_url);
        configured.api_key_env = Some("PATH".to_owned());
        configured.api_secret_env = Some("PATH".to_owned());
        configured
    }

    #[test]
    fn resolves_default_reconciliation_base_url_from_mode() {
        let paper = AlpacaConnector::new(&account(ExecutionMode::Paper));
        assert_eq!(paper.reconciliation_base_url(), ALPACA_PAPER_BASE_URL);

        let live = AlpacaConnector::new(&account(ExecutionMode::Live));
        assert_eq!(live.reconciliation_base_url(), ALPACA_LIVE_BASE_URL);
    }

    #[test]
    fn normalizes_alpaca_order_and_position_payloads() {
        let orders = serde_json::from_str::<Vec<AlpacaOrderPayload>>(
            r#"[
                {
                    "client_order_id": "abc-123",
                    "symbol": "AAPL",
                    "status": "new",
                    "qty": "2"
                }
            ]"#,
        )
        .unwrap();
        let positions = serde_json::from_str::<Vec<AlpacaPositionPayload>>(
            r#"[
                {
                    "symbol": "AAPL",
                    "qty": "1.5"
                }
            ]"#,
        )
        .unwrap();

        let snapshot = ConnectorAccountSnapshot {
            open_orders: normalize_orders(orders),
            positions: normalize_positions(positions),
            balances: normalize_account_balances(&AlpacaAccountPayload {
                cash: 750.0,
                equity: 1_000.0,
            }),
        };

        assert_eq!(snapshot.open_orders.len(), 1);
        assert_eq!(snapshot.open_orders[0].client_order_id, "abc-123");
        assert_f64_eq(snapshot.open_orders[0].quantity, 2.0);
        assert_eq!(snapshot.positions.len(), 1);
        assert_eq!(snapshot.positions[0].symbol, "AAPL");
        assert_f64_eq(snapshot.positions[0].quantity, 1.5);
        assert_eq!(snapshot.balances.len(), 2);
        assert_eq!(snapshot.balances[0].asset, "CASH");
        assert_f64_eq(snapshot.balances[0].free, 750.0);
        assert_eq!(snapshot.balances[1].asset, "EQUITY");
        assert_f64_eq(snapshot.balances[1].free, 1_000.0);
    }

    #[test]
    fn derives_symbol_constraints_from_fractionable_asset_metadata() {
        let fractionable = symbol_constraints_from_asset(&AlpacaAssetPayload {
            tradable: true,
            fractionable: true,
        });
        assert_eq!(fractionable.fractional_entry_supported, Some(true));
        assert!(fractionable.quantity_step.is_none());
        assert!(fractionable.min_quantity.is_none());
        assert!(fractionable.min_notional_usd.is_none());

        let whole_share = symbol_constraints_from_asset(&AlpacaAssetPayload {
            tradable: true,
            fractionable: false,
        });
        assert_eq!(whole_share.fractional_entry_supported, Some(false));
        assert_eq!(whole_share.quantity_step, Some(1.0));
        assert_eq!(whole_share.min_quantity, Some(1.0));
        assert!(whole_share.min_notional_usd.is_none());
    }

    #[test]
    fn maps_core_timeframes_to_alpaca_labels() {
        assert_eq!(alpaca_timeframe_label(Timeframe::M1), "1Min");
        assert_eq!(alpaca_timeframe_label(Timeframe::M5), "5Min");
        assert_eq!(alpaca_timeframe_label(Timeframe::M15), "15Min");
        assert_eq!(alpaca_timeframe_label(Timeframe::M30), "30Min");
        assert_eq!(alpaca_timeframe_label(Timeframe::H1), "1Hour");
        assert_eq!(alpaca_timeframe_label(Timeframe::H4), "4Hour");
        assert_eq!(alpaca_timeframe_label(Timeframe::D1), "1Day");
    }

    #[test]
    fn decodes_alpaca_bar_payload() {
        let payload = serde_json::from_str::<AlpacaBarsPayload>(
            r#"{
                "bars": [
                    {
                        "t": "2026-01-01T00:00:00Z",
                        "o": 100.0,
                        "h": 102.0,
                        "l": 99.5,
                        "c": 101.5,
                        "v": 1200
                    }
                ]
            }"#,
        )
        .unwrap();

        let bars = payload.bars.expect("bars should be present");

        assert_eq!(bars.len(), 1);
        assert_f64_eq(bars[0].open, 100.0);
        assert_f64_eq(bars[0].close, 101.5);
    }

    #[test]
    fn decodes_null_alpaca_bars_payload() {
        let payload = serde_json::from_str::<AlpacaBarsPayload>(
            r#"{
                "bars": null
            }"#,
        )
        .unwrap();

        assert!(payload.bars.is_none());
    }

    #[test]
    fn decodes_paginated_alpaca_historical_bars_payload() {
        let payload = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
            r#"{
                "bars": {
                    "TSLA": [
                        {
                            "t": "2026-01-01T00:00:00Z",
                            "o": 100.0,
                            "h": 102.0,
                            "l": 99.5,
                            "c": 101.5,
                            "v": 1200
                        }
                    ]
                },
                "next_page_token": "page-2"
            }"#,
        )
        .unwrap();

        let bars = historical_alpaca_bars_for_symbol(payload.bars, "TSLA");
        assert_eq!(bars.len(), 1);
        assert_eq!(payload.next_page_token.as_deref(), Some("page-2"));
        assert_f64_eq(bars[0].close, 101.5);
    }

    #[test]
    fn paginated_alpaca_recent_bars_stay_oldest_first_and_confirmed_only() {
        let page_one = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
            r#"{
                "bars": {
                    "TSLA": [
                        {
                            "t": "2026-01-01T16:00:00Z",
                            "o": 104.0,
                            "h": 105.0,
                            "l": 103.0,
                            "c": 104.5,
                            "v": 1400
                        },
                        {
                            "t": "2026-01-01T12:00:00Z",
                            "o": 103.0,
                            "h": 104.0,
                            "l": 102.0,
                            "c": 103.5,
                            "v": 1300
                        }
                    ]
                },
                "next_page_token": "page-2"
            }"#,
        )
        .unwrap();
        let page_two = serde_json::from_str::<AlpacaHistoricalBarsPayload>(
            r#"{
                "bars": {
                    "TSLA": [
                        {
                            "t": "2026-01-01T08:00:00Z",
                            "o": 102.0,
                            "h": 103.0,
                            "l": 101.0,
                            "c": 102.5,
                            "v": 1200
                        },
                        {
                            "t": "2026-01-01T04:00:00Z",
                            "o": 101.0,
                            "h": 102.0,
                            "l": 100.0,
                            "c": 101.5,
                            "v": 1100
                        }
                    ]
                },
                "next_page_token": null
            }"#,
        )
        .unwrap();

        let mut collected = historical_alpaca_bars_for_symbol(page_one.bars, "TSLA");
        collected.extend(historical_alpaca_bars_for_symbol(page_two.bars, "TSLA"));

        let now = DateTime::parse_from_rfc3339("2026-01-01T17:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let normalized = normalize_recent_alpaca_bars(collected, Timeframe::H4, now, 3);

        assert_eq!(normalized.len(), 3);
        assert_eq!(
            normalized[0].timestamp.to_rfc3339(),
            "2026-01-01T04:00:00+00:00"
        );
        assert_eq!(
            normalized[1].timestamp.to_rfc3339(),
            "2026-01-01T08:00:00+00:00"
        );
        assert_eq!(
            normalized[2].timestamp.to_rfc3339(),
            "2026-01-01T12:00:00+00:00"
        );
    }

    #[test]
    fn recent_bars_lookback_expands_for_slow_timeframes() {
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let minute_start = alpaca_recent_bars_lookback_start(now, Timeframe::M1, 200);
        let hour_start = alpaca_recent_bars_lookback_start(now, Timeframe::H4, 200);

        let minute_start = DateTime::parse_from_rfc3339(&minute_start)
            .unwrap()
            .with_timezone(&Utc);
        let hour_start = DateTime::parse_from_rfc3339(&hour_start)
            .unwrap()
            .with_timezone(&Utc);

        assert_eq!(
            (now - minute_start).num_days(),
            ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS
        );
        assert!((now - hour_start).num_days() > ALPACA_RECENT_BARS_LOOKBACK_MIN_DAYS);
    }

    #[test]
    fn recent_alpaca_bars_are_oldest_first_and_confirmed_only() {
        let bars = serde_json::from_str::<Vec<AlpacaBarPayload>>(
            r#"[
                {
                    "t": "2026-01-01T00:03:00Z",
                    "o": 103.0,
                    "h": 104.0,
                    "l": 102.0,
                    "c": 103.5,
                    "v": 1300
                },
                {
                    "t": "2026-01-01T00:02:00Z",
                    "o": 102.0,
                    "h": 103.0,
                    "l": 101.0,
                    "c": 102.5,
                    "v": 1200
                },
                {
                    "t": "2026-01-01T00:01:00Z",
                    "o": 101.0,
                    "h": 102.0,
                    "l": 100.0,
                    "c": 101.5,
                    "v": 1100
                }
            ]"#,
        )
        .unwrap();

        let now = DateTime::parse_from_rfc3339("2026-01-01T00:03:30Z")
            .unwrap()
            .with_timezone(&Utc);
        let normalized = normalize_recent_alpaca_bars(bars, Timeframe::M1, now, 2);

        assert_eq!(normalized.len(), 2);
        assert_eq!(
            normalized[0].timestamp.to_rfc3339(),
            "2026-01-01T00:01:00+00:00"
        );
        assert_eq!(
            normalized[1].timestamp.to_rfc3339(),
            "2026-01-01T00:02:00+00:00"
        );
        assert_f64_eq(normalized[0].close, 101.5);
        assert_f64_eq(normalized[1].close, 102.5);
    }

    #[test]
    fn latest_alpaca_bar_ignores_open_bar() {
        let bars = serde_json::from_str::<Vec<AlpacaBarPayload>>(
            r#"[
                {
                    "t": "2026-01-01T00:03:00Z",
                    "o": 103.0,
                    "h": 104.0,
                    "l": 102.0,
                    "c": 103.5,
                    "v": 1300
                },
                {
                    "t": "2026-01-01T00:02:00Z",
                    "o": 102.0,
                    "h": 103.0,
                    "l": 101.0,
                    "c": 102.5,
                    "v": 1200
                }
            ]"#,
        )
        .unwrap();

        let now = DateTime::parse_from_rfc3339("2026-01-01T00:03:30Z")
            .unwrap()
            .with_timezone(&Utc);
        let latest = latest_confirmed_alpaca_bar(bars, Timeframe::M1, now)
            .expect("expected a confirmed bar");

        assert_eq!(latest.timestamp.to_rfc3339(), "2026-01-01T00:02:00+00:00");
        assert_f64_eq(latest.close, 102.5);
    }

    #[test]
    fn submits_remote_market_order_when_remote_snapshot_mode_enabled() {
        let (base_url, handle) = spawn_mock_alpaca_order_server();
        let connector = AlpacaConnector::new(&remote_submission_account(base_url));
        let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let request = ExecutionRequest {
            instance_id: "aapl-instance".to_owned(),
            symbol: "AAPL".to_owned(),
            timestamp,
            intent: TradeIntent::OpenLong,
            price: 101.25,
            quantity: 2.0,
        };

        let accepted = connector.submit_order(&request).unwrap();
        assert_eq!(accepted.side, OrderSide::Buy);
        assert_eq!(accepted.order_type, OrderType::Market);
        assert_f64_eq(accepted.quantity, 2.0);
        assert_f64_eq(accepted.price, 101.25);
        assert!(accepted.fee_asset.is_none());
        assert!(accepted.fee_amount.is_none());
        assert!(accepted.fee_normalized_usd.is_none());

        let request_lines = handle.join().unwrap();
        assert!(
            request_lines
                .iter()
                .any(|line| line.starts_with("POST /v2/orders "))
        );
    }

    fn spawn_mock_alpaca_order_server() -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request_with_body(&mut stream);
            let request_lines = request.lines().map(ToOwned::to_owned).collect::<Vec<_>>();

            let response_body = r#"{
                "id": "order-1",
                "client_order_id": "alpaca-test-order-1",
                "status": "filled",
                "filled_qty": "2",
                "filled_avg_price": "101.25"
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
