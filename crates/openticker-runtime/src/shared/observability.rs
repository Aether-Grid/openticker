use crate::{
    LatencyAccumulator, RuntimeObservabilityStatus, ServiceObservability,
    saturating_duration_to_millis,
};
use std::time::Duration;

pub(crate) use openticker_lane::aggregate_bot_state;

impl ServiceObservability {
    pub(crate) fn status(&self) -> RuntimeObservabilityStatus {
        RuntimeObservabilityStatus {
            risk_rejects_total: self.risk_rejects_total,
            ledger_reserve_attempts_total: self.ledger_reserve_attempts_total,
            ledger_bot_rejects_total: self.ledger_bot_rejects_total,
            ledger_account_rejects_total: self.ledger_account_rejects_total,
            process_bar_latency_ms_last: self.process_bar_latency.last_ms,
            process_bar_latency_ms_max: self.process_bar_latency.max_ms,
            process_bar_latency_ms_avg: self.process_bar_latency.average_ms(),
            process_bar_latency_samples: self.process_bar_latency.samples,
            execution_submit_latency_ms_last: self.execution_submit_latency.last_ms,
            execution_submit_latency_ms_max: self.execution_submit_latency.max_ms,
            execution_submit_latency_ms_avg: self.execution_submit_latency.average_ms(),
            execution_submit_latency_samples: self.execution_submit_latency.samples,
        }
    }
}

impl LatencyAccumulator {
    pub(crate) fn record_elapsed(&mut self, elapsed: Duration) {
        let elapsed_ms = saturating_duration_to_millis(elapsed);
        self.last_ms = Some(elapsed_ms);
        self.max_ms = self.max_ms.max(elapsed_ms);
        self.total_ms = self.total_ms.saturating_add(u128::from(elapsed_ms));
        self.samples = self.samples.saturating_add(1);
    }

    pub(crate) fn average_ms(&self) -> Option<f64> {
        if self.samples == 0 {
            None
        } else {
            let total = self.total_ms.to_string().parse::<f64>().ok()?;
            let samples = self.samples.to_string().parse::<f64>().ok()?;
            Some(total / samples)
        }
    }
}
