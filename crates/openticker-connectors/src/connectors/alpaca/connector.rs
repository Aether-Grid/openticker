use super::account::{
    AlpacaAccountPayload, AlpacaAssetPayload, AlpacaOrderPayload, AlpacaPositionPayload,
    normalize_account_balances, normalize_orders, normalize_positions,
    symbol_constraints_from_asset,
};
use super::bars::{
    AlpacaBarsPayload, AlpacaHistoricalBarsPayload, alpaca_recent_bars_lookback_start,
    alpaca_timeframe_label, historical_alpaca_bars_for_symbol, latest_confirmed_alpaca_bar,
    normalize_confirmed_alpaca_range_bars, normalize_recent_alpaca_bars,
};
use super::http::{decode_json_response, decode_order_submission_json};
use super::orders::{
    AlpacaSubmittedOrderPayload, accepted_order_from_alpaca_payload, alpaca_order_side_label,
    alpaca_order_status_is_terminal,
};
use crate::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot, ConnectorError,
    ConnectorExecution, ConnectorHealth, ConnectorKind, ConnectorMarketData, ConnectorMarketStream,
    ConnectorPreviewStreamSession, ConnectorPrivateStream, ConnectorPrivateStreamEvent,
    ConnectorReconcile, ConnectorResiliencePolicy, ConnectorResilienceState,
    ConnectorRuntimeControl, ConnectorStatus, ConnectorSymbolConstraints,
    ConnectorSymbolConstraintsLookup, StubConnector, default_blocking_http_client, descriptor_for,
    deterministic_remote_client_order_id, format_decimal_quantity, resolve_secret_env_value,
    run_in_blocking_thread, sanitize_symbol_for_error,
};
use openticker_core::{ExecutionMode, OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, ExecutionRouter, PaperExecutionRouter,
};
use std::time::Duration;

pub(super) const ALPACA_PAPER_BASE_URL: &str = "https://paper-api.alpaca.markets";
pub(super) const ALPACA_LIVE_BASE_URL: &str = "https://api.alpaca.markets";
const ALPACA_DATA_BASE_URL: &str = "https://data.alpaca.markets";

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

    pub(super) fn reconciliation_base_url(&self) -> String {
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
