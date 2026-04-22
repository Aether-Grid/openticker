use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct RuntimeObservabilityStatus {
    pub risk_rejects_total: u64,
    pub ledger_reserve_attempts_total: u64,
    pub ledger_bot_rejects_total: u64,
    pub ledger_account_rejects_total: u64,
    pub process_bar_latency_ms_last: Option<u64>,
    pub process_bar_latency_ms_max: u64,
    pub process_bar_latency_ms_avg: Option<f64>,
    pub process_bar_latency_samples: u64,
    pub execution_submit_latency_ms_last: Option<u64>,
    pub execution_submit_latency_ms_max: u64,
    pub execution_submit_latency_ms_avg: Option<f64>,
    pub execution_submit_latency_samples: u64,
}

#[derive(Debug, Default)]
pub(crate) struct ServiceObservability {
    pub(crate) risk_rejects_total: u64,
    pub(crate) ledger_reserve_attempts_total: u64,
    pub(crate) ledger_bot_rejects_total: u64,
    pub(crate) ledger_account_rejects_total: u64,
    pub(crate) process_bar_latency: LatencyAccumulator,
    pub(crate) execution_submit_latency: LatencyAccumulator,
}

#[derive(Debug, Default)]
pub(crate) struct LatencyAccumulator {
    pub(crate) last_ms: Option<u64>,
    pub(crate) max_ms: u64,
    pub(crate) total_ms: u128,
    pub(crate) samples: u64,
}
