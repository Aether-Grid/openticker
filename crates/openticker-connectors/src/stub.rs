use crate::capabilities::descriptor_for;
use crate::error::ConnectorError;
use crate::helpers::unix_now_ms;
use crate::traits::{
    ConnectorExecution, ConnectorHealth, ConnectorMarketData, ConnectorMarketStream,
    ConnectorPrivateStream, ConnectorReconcile, ConnectorRuntimeControl,
    ConnectorSymbolConstraintsLookup,
};
use crate::types::{
    ConfirmedBarPage, ConnectionState, ConnectorAccountSnapshot, ConnectorKind,
    ConnectorPreviewStreamSession, ConnectorPrivateStreamEvent, ConnectorResilienceState,
    ConnectorStatus, ConnectorSymbolConstraints,
};
use chrono::{DateTime, Utc};
use openticker_core::{ExecutionMode, OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{
    AcceptedOrder, ExecutionRequest, ExecutionRouter, PaperExecutionRouter,
};

#[derive(Debug, Clone)]
pub struct StubConnector {
    kind: ConnectorKind,
    mode: ExecutionMode,
    use_demo_mode: bool,
    state: ConnectionState,
    resilience_state: ConnectorResilienceState,
}

impl StubConnector {
    #[must_use]
    pub fn new(kind: ConnectorKind, mode: ExecutionMode, use_demo_mode: bool) -> Self {
        Self {
            kind,
            mode,
            use_demo_mode,
            state: ConnectionState::Connected,
            resilience_state: ConnectorResilienceState::default(),
        }
    }

    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the selected execution mode is unsupported.
    pub fn validate_mode(&self) -> Result<(), ConnectorError> {
        let descriptor = descriptor_for(self.kind);
        match self.mode {
            ExecutionMode::Paper if descriptor.supports_paper => Ok(()),
            ExecutionMode::Paper if descriptor.supports_demo && self.use_demo_mode => Ok(()),
            ExecutionMode::Paper if descriptor.supports_demo => {
                Err(ConnectorError::DemoModeRequired { kind: self.kind })
            }
            ExecutionMode::Live if descriptor.supports_live => Ok(()),
            ExecutionMode::Paper | ExecutionMode::Live => Err(ConnectorError::UnsupportedMode {
                kind: self.kind,
                mode: self.mode,
            }),
        }
    }
}

fn execution_mode_label(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Paper => "paper",
        ExecutionMode::Live => "live",
    }
}

impl ConnectorHealth for StubConnector {
    fn health(&self) -> ConnectorStatus {
        ConnectorStatus {
            kind: self.kind,
            state: self.state,
            message: format!("mode={}", execution_mode_label(self.mode)),
        }
    }

    fn resilience_policy(&self) -> crate::types::ConnectorResiliencePolicy {
        descriptor_for(self.kind).resilience
    }

    fn resilience_state(&self) -> ConnectorResilienceState {
        self.resilience_state
    }
}

impl ConnectorReconcile for StubConnector {
    fn fetch_account_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        if matches!(self.state, ConnectionState::Connected) {
            Ok(ConnectorAccountSnapshot::default())
        } else {
            Err(ConnectorError::Unavailable {
                kind: self.kind,
                state: self.state,
            })
        }
    }
}

impl ConnectorMarketData for StubConnector {
    fn fetch_latest_bar(
        &self,
        _symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError> {
        let now_ms = unix_now_ms();
        let timeframe_ms = i64::try_from(timeframe.duration().as_millis()).unwrap_or(60_000);
        let bucket_start_ms = now_ms - now_ms.rem_euclid(timeframe_ms.max(1));
        let latest_confirmed_start_ms = bucket_start_ms.saturating_sub(timeframe_ms.max(1));
        let timestamp = chrono::DateTime::from_timestamp_millis(latest_confirmed_start_ms).ok_or(
            ConnectorError::RemoteSnapshot {
                kind: self.kind,
                detail: "failed to construct stub bar timestamp".to_owned(),
            },
        )?;

        let anchor = match self.kind {
            ConnectorKind::Alpaca => 150.0,
            ConnectorKind::Binance => 50_000.0,
        };
        let wave_bucket = (latest_confirmed_start_ms / timeframe_ms.max(1)).rem_euclid(10);
        let wave = f64::from(i32::try_from(wave_bucket).unwrap_or(0));
        let close = anchor + wave;

        Ok(OhlcvBar {
            timestamp,
            open: close - 0.2,
            high: close + 0.4,
            low: close - 0.5,
            close,
            volume: 1_000.0,
        })
    }

    fn fetch_recent_bars(
        &self,
        _symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let now_ms = unix_now_ms();
        let timeframe_ms = i64::try_from(timeframe.duration().as_millis()).unwrap_or(60_000);
        let bucket_start_ms = now_ms - now_ms.rem_euclid(timeframe_ms.max(1));
        let latest_confirmed_start_ms = bucket_start_ms.saturating_sub(timeframe_ms.max(1));
        let mut bars = Vec::with_capacity(limit);

        for offset in (0..limit).rev() {
            let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);
            let timestamp_ms =
                latest_confirmed_start_ms.saturating_sub(timeframe_ms.saturating_mul(offset_i64));
            let timestamp = chrono::DateTime::from_timestamp_millis(timestamp_ms).ok_or(
                ConnectorError::RemoteSnapshot {
                    kind: self.kind,
                    detail: "failed to construct stub historical bar timestamp".to_owned(),
                },
            )?;

            let anchor = match self.kind {
                ConnectorKind::Alpaca => 150.0,
                ConnectorKind::Binance => 50_000.0,
            };
            let wave_bucket = (timestamp_ms / timeframe_ms.max(1)).rem_euclid(10);
            let wave = f64::from(i32::try_from(wave_bucket).unwrap_or(0));
            let close = anchor + wave;

            bars.push(OhlcvBar {
                timestamp,
                open: close - 0.2,
                high: close + 0.4,
                low: close - 0.5,
                close,
                volume: 1_000.0,
            });
        }

        Ok(bars)
    }

    fn fetch_latest_confirmed_bar_timestamp(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, ConnectorError> {
        self.fetch_latest_bar(symbol, timeframe)
            .map(|bar| Some(bar.timestamp))
    }

    fn fetch_confirmed_bars_range(
        &self,
        _symbol: &str,
        timeframe: Timeframe,
        start_after: Option<DateTime<Utc>>,
        end_at: DateTime<Utc>,
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

        let timeframe_ms = i64::try_from(timeframe.duration().as_millis())
            .unwrap_or(60_000)
            .max(1);
        let now_ms = unix_now_ms();
        let bucket_start_ms = now_ms - now_ms.rem_euclid(timeframe_ms);
        let latest_confirmed_start_ms = bucket_start_ms.saturating_sub(timeframe_ms);
        let effective_end_ms = end_at.timestamp_millis().min(latest_confirmed_start_ms);
        let requested = limit.max(1);
        let first_timestamp_ms = start_after.map_or_else(
            || {
                let span = i64::try_from(requested.saturating_sub(1)).unwrap_or(i64::MAX);
                effective_end_ms.saturating_sub(timeframe_ms.saturating_mul(span))
            },
            |timestamp| timestamp.timestamp_millis().saturating_add(timeframe_ms),
        );

        if first_timestamp_ms > effective_end_ms {
            return Ok(ConfirmedBarPage {
                bars: Vec::new(),
                exhausted: true,
            });
        }

        let mut bars = Vec::with_capacity(requested);
        let mut current_ms = first_timestamp_ms;
        while current_ms <= effective_end_ms && bars.len() < requested {
            let timestamp = chrono::DateTime::from_timestamp_millis(current_ms).ok_or(
                ConnectorError::RemoteSnapshot {
                    kind: self.kind,
                    detail: "failed to construct stub recovery bar timestamp".to_owned(),
                },
            )?;

            let anchor = match self.kind {
                ConnectorKind::Alpaca => 150.0,
                ConnectorKind::Binance => 50_000.0,
            };
            let wave_bucket = (current_ms / timeframe_ms).rem_euclid(10);
            let wave = f64::from(i32::try_from(wave_bucket).unwrap_or(0));
            let close = anchor + wave;

            bars.push(OhlcvBar {
                timestamp,
                open: close - 0.2,
                high: close + 0.4,
                low: close - 0.5,
                close,
                volume: 1_000.0,
            });

            current_ms = current_ms.saturating_add(timeframe_ms);
        }

        Ok(ConfirmedBarPage {
            exhausted: bars
                .last()
                .is_some_and(|bar| bar.timestamp.timestamp_millis() >= effective_end_ms),
            bars,
        })
    }
}

impl ConnectorExecution for StubConnector {
    fn submit_order(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ConnectorError> {
        if !matches!(self.state, ConnectionState::Connected) {
            return Err(ConnectorError::Unavailable {
                kind: self.kind,
                state: self.state,
            });
        }

        let mut accepted = PaperExecutionRouter.submit(request).map_err(|error| {
            ConnectorError::OrderSubmission {
                kind: self.kind,
                detail: error.to_string(),
            }
        })?;
        accepted.client_order_id = format!("{}-{}", self.kind.as_str(), accepted.client_order_id);
        Ok(accepted)
    }
}

impl ConnectorSymbolConstraintsLookup for StubConnector {
    fn fetch_symbol_constraints(
        &self,
        _symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        Ok(ConnectorSymbolConstraints::default())
    }
}

impl ConnectorMarketStream for StubConnector {
    fn normalize_market_data_event(
        &self,
        _payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
        Ok(None)
    }

    fn start_preview_stream_session(
        &self,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError> {
        Ok(None)
    }
}

impl ConnectorPrivateStream for StubConnector {
    fn normalize_private_stream_event(
        &self,
        _payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
        Ok(None)
    }
}

impl ConnectorRuntimeControl for StubConnector {
    fn note_disconnect(&mut self, now_ms: i64) {
        self.resilience_state
            .note_disconnect(now_ms, descriptor_for(self.kind).resilience);
    }

    fn note_reconnect_success(&mut self) {
        self.resilience_state.note_reconnect_success();
    }

    fn note_rate_limit(&mut self, now_ms: i64, throttle_window_ms: u64) {
        self.resilience_state
            .note_rate_limit(now_ms, throttle_window_ms);
    }
}
