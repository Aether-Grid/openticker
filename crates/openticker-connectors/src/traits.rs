use crate::error::ConnectorError;
use crate::types::{
    ConfirmedBarPage, ConnectorAccountSnapshot, ConnectorPreviewStreamSession,
    ConnectorPrivateStreamEvent, ConnectorResiliencePolicy, ConnectorResilienceState,
    ConnectorStatus, ConnectorSymbolConstraints,
};
use chrono::{DateTime, Utc};
use openticker_core::{OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{AcceptedOrder, ExecutionRequest};

pub trait ConnectorHealth {
    fn health(&self) -> ConnectorStatus;
    fn resilience_policy(&self) -> ConnectorResiliencePolicy;
    fn resilience_state(&self) -> ConnectorResilienceState;
}

pub trait ConnectorReconcile {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the connector cannot provide an account snapshot.
    fn fetch_account_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError>;
}

pub trait ConnectorMarketData {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when latest market data cannot be fetched.
    fn fetch_latest_bar(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError>;

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when recent market data cannot be fetched.
    fn fetch_recent_bars(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError>;

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when latest confirmed market data cannot be fetched.
    fn fetch_latest_confirmed_bar_timestamp(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, ConnectorError>;

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when confirmed historical bar retrieval fails.
    fn fetch_confirmed_bars_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<DateTime<Utc>>,
        end_at: DateTime<Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ConnectorError>;
}

pub trait ConnectorExecution {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when an order cannot be submitted.
    fn submit_order(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ConnectorError>;
}

pub trait ConnectorSymbolConstraintsLookup {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when symbol constraints cannot be fetched.
    fn fetch_symbol_constraints(
        &self,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError>;
}

pub trait ConnectorMarketStream {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when stream payload decoding fails.
    fn normalize_market_data_event(
        &self,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError>;

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when preview-stream setup fails.
    fn start_preview_stream_session(
        &self,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError>;
}

pub trait ConnectorPrivateStream {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when stream payload decoding fails.
    fn normalize_private_stream_event(
        &self,
        payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError>;
}

pub trait ConnectorRuntimeControl {
    fn note_disconnect(&mut self, now_ms: i64);
    fn note_reconnect_success(&mut self);
    fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64);
}

pub trait ConnectorClient:
    ConnectorHealth
    + ConnectorReconcile
    + ConnectorMarketData
    + ConnectorExecution
    + ConnectorSymbolConstraintsLookup
    + ConnectorMarketStream
    + ConnectorPrivateStream
    + ConnectorRuntimeControl
    + std::fmt::Debug
    + Send
    + Sync
{
}

impl<T> ConnectorClient for T where
    T: ConnectorHealth
        + ConnectorReconcile
        + ConnectorMarketData
        + ConnectorExecution
        + ConnectorSymbolConstraintsLookup
        + ConnectorMarketStream
        + ConnectorPrivateStream
        + ConnectorRuntimeControl
        + std::fmt::Debug
        + Send
        + Sync
{
}
