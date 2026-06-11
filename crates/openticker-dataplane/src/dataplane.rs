use crate::buffer::StreamBuffer;
use crate::metrics::{DataPlaneMetrics, DataPlaneMetricsSnapshot};
use crate::registry::StreamRegistry;
use crate::stream::{
    StreamKey, StreamPreviewConnectionState, StreamSource, StreamSpec, StreamStatus,
    StreamUpdateSource, compare_stream_key,
};
use openticker_core::OhlcvBar;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::watch;
use tokio::time::sleep;

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

#[derive(Debug, Clone)]
struct StreamEntry {
    spec: StreamSpec,
    buffer: StreamBuffer,
    last_attempt_ms: Option<i64>,
    last_success_ms: Option<i64>,
    last_error: Option<String>,
    preview_connection_state: Option<StreamPreviewConnectionState>,
    last_preview_update_ms: Option<i64>,
    latest_preview_bar: Option<OhlcvBar>,
    last_preview_error: Option<String>,
    last_confirmed_update_source: Option<StreamUpdateSource>,
    fetch_count: u64,
    error_count: u64,
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

    pub async fn run_forever<
        Attempt,
        AttemptFut,
        Fetch,
        FetchFut,
        Success,
        SuccessFut,
        ErrorCb,
        ErrorFut,
    >(
        &self,
        interval_ms: u64,
        mut shutdown: watch::Receiver<bool>,
        mut on_attempt: Attempt,
        mut fetcher: Fetch,
        mut on_success: Success,
        mut on_error: ErrorCb,
    ) where
        Attempt: FnMut(&StreamKey, i64) -> AttemptFut,
        AttemptFut: Future<Output = Result<(), String>>,
        Fetch: FnMut(&StreamKey) -> FetchFut,
        FetchFut: Future<Output = Result<OhlcvBar, String>>,
        Success: FnMut(&StreamKey, &OhlcvBar, i64, bool) -> SuccessFut,
        SuccessFut: Future<Output = Result<(), String>>,
        ErrorCb: FnMut(&StreamKey, &str) -> ErrorFut,
        ErrorFut: Future<Output = Result<(), String>>,
    {
        loop {
            if *shutdown.borrow() {
                break;
            }

            let cycle_started_at = Instant::now();
            let now_ms = unix_now_ms();
            let due_streams = self.take_due_streams(now_ms);

            for stream_key in due_streams {
                let attempt_result = on_attempt(&stream_key, now_ms).await;
                if let Err(error) = attempt_result {
                    // The stream may have been unregistered between selection
                    // and error handling; count any dropped failure so it is
                    // observable instead of vanishing, but keep the loop going.
                    if self.record_fetch_error(&stream_key, &error).is_err() {
                        self.record_dropped_error_record();
                    }
                    self.record_completion(true);
                    if on_error(&stream_key, &error).await.is_err() {
                        self.record_dropped_error_record();
                    }
                    continue;
                }

                let fetch_started_at = Instant::now();
                let fetched_bar = fetcher(&stream_key).await;
                self.record_connector_fetch_latency(fetch_started_at.elapsed());

                let completion_result = match fetched_bar {
                    Ok(bar) => match self.record_fetched_bar(&stream_key, now_ms, bar.clone()) {
                        Ok(appended) => on_success(&stream_key, &bar, now_ms, appended).await,
                        Err(error) => Err(error.to_string()),
                    },
                    Err(error) => {
                        // As above: surface dropped error records (e.g. for a
                        // concurrently unregistered stream) via a counter
                        // rather than discarding them silently.
                        if self.record_fetch_error(&stream_key, &error).is_err() {
                            self.record_dropped_error_record();
                        }
                        if on_error(&stream_key, &error).await.is_err() {
                            self.record_dropped_error_record();
                        }
                        Err(error)
                    }
                };

                self.record_completion(completion_result.is_err());
            }

            self.record_cycle_duration(cycle_started_at.elapsed());

            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                () = sleep(Duration::from_millis(interval_ms)) => {}
            }
        }
    }
}

impl StreamEntry {
    fn new(spec: StreamSpec) -> Self {
        let retention = spec.retention;
        Self {
            spec,
            buffer: StreamBuffer::new(retention),
            last_attempt_ms: None,
            last_success_ms: None,
            last_error: None,
            preview_connection_state: None,
            last_preview_update_ms: None,
            latest_preview_bar: None,
            last_preview_error: None,
            last_confirmed_update_source: None,
            fetch_count: 0,
            error_count: 0,
        }
    }

    fn status(&self, now_ms: i64, sparkline_limit: usize) -> StreamStatus {
        let transport_staleness_ms = self.last_success_ms.map(|last_success_ms| {
            u64::try_from(now_ms.saturating_sub(last_success_ms)).unwrap_or(0)
        });
        let latest_bar = self.buffer.latest();
        let confirmed_bar_close_ms = latest_bar.as_ref().map(|bar| {
            let timeframe_ms =
                saturating_millis_to_i64(self.spec.key.timeframe.duration().as_millis());
            bar.timestamp
                .timestamp_millis()
                .saturating_add(timeframe_ms)
        });
        let confirmed_bar_staleness_ms = confirmed_bar_close_ms
            .map(|close_ms| u64::try_from(now_ms.saturating_sub(close_ms)).unwrap_or(0));
        let confirmed_bar_stale_deadline_ms = confirmed_bar_close_ms.and_then(|close_ms| {
            self.spec.close_poll_grace_ms.map(|grace_ms| {
                let grace_ms = saturating_millis_to_i64(grace_ms);
                close_ms.saturating_add(grace_ms)
            })
        });
        let attached_instances = self
            .spec
            .sources
            .iter()
            .filter_map(|source| match source {
                StreamSource::Instance(instance_id) => Some(instance_id.clone()),
                StreamSource::Watchlist => None,
            })
            .collect::<Vec<_>>();

        StreamStatus {
            key: self.spec.key.clone(),
            retention: self.spec.retention,
            polling_interval_ms: self.spec.polling_interval_ms,
            close_poll_retry_ms: self.spec.close_poll_retry_ms,
            close_poll_grace_ms: self.spec.close_poll_grace_ms,
            last_attempt_ms: self.last_attempt_ms,
            last_success_ms: self.last_success_ms,
            last_error: self.last_error.clone(),
            latest_bar,
            fetch_count: self.fetch_count,
            error_count: self.error_count,
            transport_staleness_ms,
            staleness_ms: transport_staleness_ms,
            confirmed_bar_close_ms,
            confirmed_bar_staleness_ms,
            confirmed_bar_stale_deadline_ms,
            latest_preview_bar: self.latest_preview_bar.clone(),
            preview_enabled: self.spec.preview_enabled,
            preview_connection_state: self.preview_connection_state,
            last_preview_update_ms: self.last_preview_update_ms,
            last_preview_error: self.last_preview_error.clone(),
            last_confirmed_update_source: self.last_confirmed_update_source,
            attached_instances,
            sparkline: self.buffer.sparkline(sparkline_limit),
        }
    }

    fn is_due(&self, now_ms: i64) -> bool {
        let polling_interval_ms = saturating_millis_to_i64(self.spec.polling_interval_ms);
        let normal_due = self.last_attempt_ms.is_none_or(|last_attempt_ms| {
            now_ms.saturating_sub(last_attempt_ms) >= polling_interval_ms
        });
        normal_due || self.close_window_due(now_ms)
    }

    fn close_window_due(&self, now_ms: i64) -> bool {
        let Some(retry_ms) = self.spec.close_poll_retry_ms else {
            return false;
        };
        let Some(grace_ms) = self.spec.close_poll_grace_ms else {
            return false;
        };
        let Some(latest_bar) = self.buffer.latest() else {
            return false;
        };

        let timeframe_ms = saturating_millis_to_i64(self.spec.key.timeframe.duration().as_millis());
        let grace_ms = saturating_millis_to_i64(grace_ms);
        let expected_close_ms = latest_bar
            .timestamp
            .timestamp_millis()
            .saturating_add(timeframe_ms);
        let stale_deadline_ms = expected_close_ms.saturating_add(grace_ms);
        if now_ms < expected_close_ms || now_ms > stale_deadline_ms {
            return false;
        }

        let retry_ms = saturating_millis_to_i64(retry_ms);
        self.last_attempt_ms
            .is_none_or(|last_attempt_ms| now_ms.saturating_sub(last_attempt_ms) >= retry_ms)
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

fn unix_now_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    saturating_millis_to_i64(duration.as_millis())
}
