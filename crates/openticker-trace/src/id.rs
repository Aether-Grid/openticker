use openticker_core::SignalPhase;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleTriggerKind {
    MarketBar,
    ManualSignal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceIdentity {
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: SignalPhase,
    pub trigger_kind: CycleTriggerKind,
}

impl TraceIdentity {
    #[must_use]
    pub fn new(
        bot_id: impl Into<String>,
        symbol: impl Into<String>,
        bar_timestamp: impl Into<String>,
        phase: SignalPhase,
        trigger_kind: CycleTriggerKind,
    ) -> Self {
        Self {
            trace_id: generate_trace_id(),
            bot_id: bot_id.into(),
            symbol: symbol.into(),
            bar_timestamp: bar_timestamp.into(),
            phase,
            trigger_kind,
        }
    }

    #[must_use]
    pub fn with_trace_id(
        trace_id: impl Into<String>,
        bot_id: impl Into<String>,
        symbol: impl Into<String>,
        bar_timestamp: impl Into<String>,
        phase: SignalPhase,
        trigger_kind: CycleTriggerKind,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            bot_id: bot_id.into(),
            symbol: symbol.into(),
            bar_timestamp: bar_timestamp.into(),
            phase,
            trigger_kind,
        }
    }
}

/// Generates a short-term diagnostic trace ID of the form
/// `trc_{nanos:x}_{sequence:x}`.
///
/// IDs are unique within a single process run: the atomic `TRACE_SEQUENCE`
/// guarantees no two IDs collide while the process lives. They are **not**
/// globally unique across restarts — the sequence resets to 1 on each start,
/// and although the nanosecond timestamp prefix makes a same-nanosecond
/// restart collision improbable, it is not impossible under frequent restarts.
/// This is acceptable for short-lived diagnostic correlation; add a process ID
/// (or a UUID) if long-term cross-restart uniqueness is ever required.
#[must_use]
pub fn generate_trace_id() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let sequence = TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("trc_{:x}_{sequence:x}", duration.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_trace_ids_are_opaque_and_unique() {
        let first = generate_trace_id();
        let second = generate_trace_id();

        assert!(first.starts_with("trc_"));
        assert!(second.starts_with("trc_"));
        assert_ne!(first, second);
    }
}
