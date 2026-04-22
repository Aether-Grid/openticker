use crate::{CycleTrace, CycleTraceSummary, Runtime, ServiceError};
use serde::de::DeserializeOwned;

impl Runtime {
    /// Returns recent cycle summaries for a bot with optional lane and outcome filters.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval or summary decoding fails.
    pub fn recent_cycle_traces_for_bot(
        &self,
        bot_id: &str,
        symbol: Option<&str>,
        phase: Option<&str>,
        outcome: Option<&str>,
        bar_timestamp: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CycleTraceSummary>, ServiceError> {
        self.repo()
            .recent_cycle_traces_for_bot(bot_id, symbol, phase, outcome, bar_timestamp, limit)?
            .into_iter()
            .map(|record| cycle_trace_summary_from_record(&record))
            .collect()
    }

    /// Returns one authoritative cycle trace for a bot and `trace_id`.
    ///
    /// # Errors
    ///
    /// Returns an error when storage retrieval or payload decoding fails.
    pub fn cycle_trace_for_bot(
        &self,
        bot_id: &str,
        trace_id: &str,
    ) -> Result<Option<CycleTrace>, ServiceError> {
        let Some(record) = self.repo().cycle_trace_by_id(trace_id)? else {
            return Ok(None);
        };
        if record.bot_id != bot_id {
            return Ok(None);
        }

        let mut detail = serde_json::from_str::<CycleTrace>(&record.payload_json)?;
        detail.summary = cycle_trace_summary_from_record(&record)?;
        Ok(Some(detail))
    }
}

pub(super) fn cycle_trace_summary_from_record(
    record: &crate::CycleTraceRecord,
) -> Result<CycleTraceSummary, ServiceError> {
    Ok(CycleTraceSummary {
        trace_id: record.trace_id.clone(),
        bot_id: record.bot_id.clone(),
        symbol: record.symbol.clone(),
        bar_timestamp: record.bar_timestamp.clone(),
        phase: parse_json_label(&record.phase)?,
        trigger_kind: parse_json_label(&record.trigger_kind)?,
        signal: parse_json_label(&record.signal)?,
        intent: parse_json_label(&record.intent)?,
        risk_decision: parse_json_label(&record.risk_decision)?,
        outcome: parse_json_label(&record.outcome)?,
        created_at_ms: record.created_at_ms,
    })
}

pub(super) fn parse_json_label<T: DeserializeOwned>(value: &str) -> Result<T, ServiceError> {
    Ok(serde_json::from_str::<T>(&format!("\"{value}\""))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleRiskDecisionLabel;
    use crate::test_support::fixture_bundle;
    use openticker_core::{IndicatorSignal, SignalPhase, TradeIntent};

    #[test]
    fn recent_cycle_traces_decode_summary_fields() {
        let mut runtime = Runtime::from_config(&fixture_bundle());
        runtime.start_instance("aapl").expect("bot should start");

        let bar = crate::test_support::test_bar_at("2030-01-01T00:00:00Z", 123.45);
        let _ = runtime
            .process_manual_signal_for_lane(
                "aapl",
                IndicatorSignal::BuyConfirmed,
                bar.close,
                bar.timestamp,
            )
            .expect("manual signal should persist a cycle trace");

        let summaries = runtime
            .recent_cycle_traces_for_bot("aapl", Some("AAPL"), None, None, None, 10)
            .expect("cycle summaries should load");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].phase, SignalPhase::Confirmed);
        assert_eq!(summaries[0].signal, IndicatorSignal::BuyConfirmed);
        assert_eq!(summaries[0].intent, TradeIntent::OpenLong);
        assert_eq!(summaries[0].risk_decision, CycleRiskDecisionLabel::Allowed);
    }
}
