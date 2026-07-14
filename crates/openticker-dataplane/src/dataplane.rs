mod entry;
mod run_loop;

use self::entry::StreamEntry;
use crate::metrics::{DataPlaneMetrics, DataPlaneMetricsSnapshot};
use crate::registry::StreamRegistry;
use crate::stream::{
    StreamKey, StreamPreviewConnectionState, StreamSpec, StreamStatus, StreamUpdateSource,
    compare_stream_key,
};
use openticker_core::OhlcvBar;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug)]
pub struct DataPlane {
    state: Mutex<DataPlaneState>,
    metrics: DataPlaneMetrics,
}

#[derive(Debug)]
struct DataPlaneState {
    registry: StreamRegistry,
    streams: HashMap<StreamKey, StreamEntry>,
}

#[derive(Debug, Error)]
pub enum DataPlaneError {
    #[error(
        "stream `{account_id}/{symbol}/{timeframe}` is not registered",
        account_id = .0.account_id,
        symbol = .0.symbol,
        timeframe = .0.timeframe,
    )]
    UnknownStream(StreamKey),
}

impl DataPlane {
    #[must_use]
    pub fn new(specs: impl IntoIterator<Item = StreamSpec>) -> Self {
        let registry = StreamRegistry::from_specs(specs);
        let streams = registry
            .specs()
            .into_iter()
            .map(|spec| (spec.key.clone(), StreamEntry::new(spec)))
            .collect();
        Self {
            state: Mutex::new(DataPlaneState { registry, streams }),
            metrics: DataPlaneMetrics::default(),
        }
    }

    /// Replaces the registry and keeps existing buffers for streams that remain registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn replace_streams(&self, specs: impl IntoIterator<Item = StreamSpec>) {
        let registry = StreamRegistry::from_specs(specs);
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let mut next_streams = HashMap::new();

        for spec in registry.specs() {
            let mut entry = state
                .streams
                .remove(&spec.key)
                .unwrap_or_else(|| StreamEntry::new(spec.clone()));
            entry.spec = spec.clone();
            entry.buffer.set_retention(spec.retention);
            next_streams.insert(spec.key.clone(), entry);
        }

        state.registry = registry;
        state.streams = next_streams;
    }

    #[must_use]
    /// Returns currently registered stream specifications.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn registered_streams(&self) -> Vec<StreamSpec> {
        let state = self.state.lock().expect("dataplane state lock poisoned");
        state.registry.specs()
    }

    #[must_use]
    /// Returns stream keys that are currently due for polling.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn take_due_streams(&self, now_ms: i64) -> Vec<StreamKey> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let mut due = Vec::new();

        for (key, entry) in &mut state.streams {
            let is_due = entry.is_due(now_ms);
            if !is_due {
                continue;
            }

            entry.last_attempt_ms = Some(now_ms);
            entry.fetch_count = entry.fetch_count.saturating_add(1);
            due.push(key.clone());
        }

        due.sort_by(compare_stream_key);
        self.metrics.record_due_requests(due.len());
        for _ in &due {
            self.metrics.record_fetch();
        }
        due
    }

    /// Stores a fetched bar for an already-registered stream.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_fetched_bar(
        &self,
        key: &StreamKey,
        now_ms: i64,
        bar: OhlcvBar,
    ) -> Result<bool, DataPlaneError> {
        self.record_fetched_bar_from_source(key, now_ms, bar, StreamUpdateSource::Poll)
    }

    /// Stores a fetched confirmed bar and tracks where the close came from.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_fetched_bar_from_source(
        &self,
        key: &StreamKey,
        now_ms: i64,
        bar: OhlcvBar,
        source: StreamUpdateSource,
    ) -> Result<bool, DataPlaneError> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get_mut(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        entry.last_success_ms = Some(now_ms);
        entry.last_error = None;
        let appended = entry.buffer.push_if_newer(bar);
        if appended {
            entry.last_confirmed_update_source = Some(source);
        }
        Ok(appended)
    }

    /// Records a fetch error for an already-registered stream.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_fetch_error(
        &self,
        key: &StreamKey,
        error: &impl ToString,
    ) -> Result<(), DataPlaneError> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get_mut(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        entry.error_count = entry.error_count.saturating_add(1);
        entry.last_error = Some(error.to_string());
        Ok(())
    }

    /// Records preview-stream health for a registered stream.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_preview_connection_state(
        &self,
        key: &StreamKey,
        state_value: StreamPreviewConnectionState,
        detail: Option<String>,
    ) -> Result<(), DataPlaneError> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get_mut(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        entry.preview_connection_state = Some(state_value);
        entry.last_preview_error = detail;
        Ok(())
    }

    /// Records that a preview update was observed for a registered stream.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_preview_update(
        &self,
        key: &StreamKey,
        now_ms: i64,
        bar: OhlcvBar,
    ) -> Result<(), DataPlaneError> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get_mut(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        entry.last_preview_update_ms = Some(now_ms);
        entry.latest_preview_bar = Some(bar);
        entry.preview_connection_state = Some(StreamPreviewConnectionState::Connected);
        entry.last_preview_error = None;
        Ok(())
    }

    /// Returns the newest bars for a stream up to the requested limit.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn snapshot_bars(
        &self,
        key: &StreamKey,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, DataPlaneError> {
        let state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        Ok(entry
            .buffer
            .snapshot(limit.min(entry.spec.retention).max(1)))
    }

    #[must_use]
    /// Returns current status snapshots for all registered streams.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn snapshot_streams(&self, now_ms: i64, sparkline_limit: usize) -> Vec<StreamStatus> {
        let state = self.state.lock().expect("dataplane state lock poisoned");
        let mut streams = state
            .streams
            .values()
            .map(|entry| entry.status(now_ms, sparkline_limit))
            .collect::<Vec<_>>();
        streams.sort_by(|left, right| compare_stream_key(&left.key, &right.key));
        streams
    }

    #[must_use]
    pub fn metrics_snapshot(&self) -> DataPlaneMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub fn record_cycle_duration(&self, duration: Duration) {
        self.metrics.record_cycle_duration(duration);
    }

    pub fn record_connector_fetch_latency(&self, duration: Duration) {
        self.metrics.record_connector_fetch_latency(duration);
    }

    pub fn record_runtime_write_lock_wait(&self, duration: Duration) {
        self.metrics.record_runtime_write_lock_wait(duration);
    }

    pub fn record_completion(&self, is_error: bool) {
        self.metrics.record_completion(is_error);
    }

    /// Records that a fetch-error or error-callback failure could not be
    /// recorded against its stream and was therefore dropped by the polling
    /// loop (for example because the stream was unregistered concurrently).
    pub fn record_dropped_error_record(&self) {
        self.metrics.record_dropped_error_record();
    }

    /// Marks a stream as manually polled and increments fetch counters.
    ///
    /// # Errors
    ///
    /// Returns [`DataPlaneError::UnknownStream`] when the stream key is not registered.
    ///
    /// # Panics
    ///
    /// Panics if the internal dataplane state mutex is poisoned.
    pub fn record_manual_poll_attempt(
        &self,
        key: &StreamKey,
        now_ms: i64,
    ) -> Result<(), DataPlaneError> {
        let mut state = self.state.lock().expect("dataplane state lock poisoned");
        let entry = state
            .streams
            .get_mut(key)
            .ok_or_else(|| DataPlaneError::UnknownStream(key.clone()))?;
        entry.last_attempt_ms = Some(now_ms);
        entry.fetch_count = entry.fetch_count.saturating_add(1);
        self.metrics.record_fetch();
        Ok(())
    }
}

/// Converts an unsigned millisecond duration into the signed `i64` millisecond
/// representation used for wall-clock timestamps, saturating at [`i64::MAX`] on
/// overflow.
///
/// The saturation is documentation, not a real failure mode: `i64::MAX`
/// milliseconds is roughly 292 million years, so polling intervals, grace
/// windows, retry windows, and timeframe durations can never approach it. The
/// cap exists only to keep the conversion total (infallible) without an
/// `unwrap`/`expect` that could panic. If an input ever did overflow, the
/// resulting timestamp would simply be pushed far into the future, which the
/// `saturating_add` callers treat as "not yet due".
fn saturating_millis_to_i64<T: TryInto<i64>>(millis: T) -> i64 {
    millis.try_into().unwrap_or(i64::MAX)
}
