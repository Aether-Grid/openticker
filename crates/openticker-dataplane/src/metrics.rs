use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct DataPlaneMetrics {
    cycle_latency: AtomicLatencyMetric,
    connector_fetch_latency: AtomicLatencyMetric,
    runtime_write_lock_wait: AtomicLatencyMetric,
    due_requests_last: AtomicU64,
    due_requests_total: AtomicU64,
    fetches_total: AtomicU64,
    completions_total: AtomicU64,
    completion_errors_total: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DataPlaneMetricsSnapshot {
    pub cycle_latency: LatencyMetricSnapshot,
    pub connector_fetch_latency: LatencyMetricSnapshot,
    pub runtime_write_lock_wait: LatencyMetricSnapshot,
    pub due_requests_last: u64,
    pub due_requests_total: u64,
    pub fetches_total: u64,
    pub completions_total: u64,
    pub completion_errors_total: u64,
}

#[derive(Debug, Default)]
pub struct AtomicLatencyMetric {
    last_ms: AtomicU64,
    max_ms: AtomicU64,
    total_ms: AtomicU64,
    samples: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LatencyMetricSnapshot {
    pub last_ms: u64,
    pub max_ms: u64,
    pub avg_ms: f64,
    pub samples: u64,
}

impl DataPlaneMetrics {
    pub(crate) fn record_cycle_duration(&self, duration: Duration) {
        self.cycle_latency.record_duration(duration);
    }

    pub(crate) fn record_connector_fetch_latency(&self, duration: Duration) {
        self.connector_fetch_latency.record_duration(duration);
    }

    pub(crate) fn record_runtime_write_lock_wait(&self, duration: Duration) {
        self.runtime_write_lock_wait.record_duration(duration);
    }

    pub(crate) fn record_due_requests(&self, due_requests: usize) {
        let due_requests = u64::try_from(due_requests).unwrap_or(u64::MAX);
        self.due_requests_last
            .store(due_requests, AtomicOrdering::Relaxed);
        let _ = self.due_requests_total.fetch_update(
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
            |total| Some(total.saturating_add(due_requests)),
        );
    }

    pub(crate) fn record_fetch(&self) {
        let _ = self.fetches_total.fetch_update(
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
            |total| Some(total.saturating_add(1)),
        );
    }

    pub(crate) fn record_completion(&self, is_error: bool) {
        let _ = self.completions_total.fetch_update(
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
            |total| Some(total.saturating_add(1)),
        );
        if is_error {
            let _ = self.completion_errors_total.fetch_update(
                AtomicOrdering::Relaxed,
                AtomicOrdering::Relaxed,
                |total| Some(total.saturating_add(1)),
            );
        }
    }

    #[must_use]
    pub(crate) fn snapshot(&self) -> DataPlaneMetricsSnapshot {
        DataPlaneMetricsSnapshot {
            cycle_latency: self.cycle_latency.snapshot(),
            connector_fetch_latency: self.connector_fetch_latency.snapshot(),
            runtime_write_lock_wait: self.runtime_write_lock_wait.snapshot(),
            due_requests_last: self.due_requests_last.load(AtomicOrdering::Relaxed),
            due_requests_total: self.due_requests_total.load(AtomicOrdering::Relaxed),
            fetches_total: self.fetches_total.load(AtomicOrdering::Relaxed),
            completions_total: self.completions_total.load(AtomicOrdering::Relaxed),
            completion_errors_total: self.completion_errors_total.load(AtomicOrdering::Relaxed),
        }
    }
}

impl AtomicLatencyMetric {
    fn record_duration(&self, duration: Duration) {
        let elapsed_ms = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        self.last_ms.store(elapsed_ms, AtomicOrdering::Relaxed);
        let _ =
            self.total_ms
                .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |total| {
                    Some(total.saturating_add(elapsed_ms))
                });
        let _ = self.samples.fetch_update(
            AtomicOrdering::Relaxed,
            AtomicOrdering::Relaxed,
            |samples| Some(samples.saturating_add(1)),
        );

        let mut current_max = self.max_ms.load(AtomicOrdering::Relaxed);
        while elapsed_ms > current_max {
            match self.max_ms.compare_exchange_weak(
                current_max,
                elapsed_ms,
                AtomicOrdering::Relaxed,
                AtomicOrdering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current_max = observed,
            }
        }
    }

    fn snapshot(&self) -> LatencyMetricSnapshot {
        let total_ms = self.total_ms.load(AtomicOrdering::Relaxed);
        let samples = self.samples.load(AtomicOrdering::Relaxed);
        LatencyMetricSnapshot {
            last_ms: self.last_ms.load(AtomicOrdering::Relaxed),
            max_ms: self.max_ms.load(AtomicOrdering::Relaxed),
            avg_ms: if samples == 0 {
                0.0
            } else {
                let total = total_ms.to_string().parse::<f64>().unwrap_or(0.0);
                let samples = samples.to_string().parse::<f64>().unwrap_or(1.0);
                total / samples
            },
            samples,
        }
    }
}
