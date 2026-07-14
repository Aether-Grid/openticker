use super::saturating_millis_to_i64;
use crate::buffer::StreamBuffer;
use crate::stream::{
    StreamPreviewConnectionState, StreamSource, StreamSpec, StreamStatus, StreamUpdateSource,
};
use openticker_core::OhlcvBar;

#[derive(Debug, Clone)]
pub(super) struct StreamEntry {
    pub(super) spec: StreamSpec,
    pub(super) buffer: StreamBuffer,
    pub(super) last_attempt_ms: Option<i64>,
    pub(super) last_success_ms: Option<i64>,
    pub(super) last_error: Option<String>,
    pub(super) preview_connection_state: Option<StreamPreviewConnectionState>,
    pub(super) last_preview_update_ms: Option<i64>,
    pub(super) latest_preview_bar: Option<OhlcvBar>,
    pub(super) last_preview_error: Option<String>,
    pub(super) last_confirmed_update_source: Option<StreamUpdateSource>,
    pub(super) fetch_count: u64,
    pub(super) error_count: u64,
}

impl StreamEntry {
    pub(super) fn new(spec: StreamSpec) -> Self {
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

    pub(super) fn status(&self, now_ms: i64, sparkline_limit: usize) -> StreamStatus {
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

    pub(super) fn is_due(&self, now_ms: i64) -> bool {
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
