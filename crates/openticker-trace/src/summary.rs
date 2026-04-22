use crate::id::{CycleTriggerKind, TraceIdentity};
use openticker_core::{IndicatorSignal, TradeIntent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleOutcome {
    NoOp,
    RiskRejected,
    AcceptedNoFill,
    AcceptedPartiallyFilled,
    AcceptedFilled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CycleRiskDecisionLabel {
    Allowed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleTraceSummary {
    pub trace_id: String,
    pub bot_id: String,
    pub symbol: String,
    pub bar_timestamp: String,
    pub phase: openticker_core::SignalPhase,
    pub trigger_kind: CycleTriggerKind,
    pub signal: IndicatorSignal,
    pub intent: TradeIntent,
    pub risk_decision: CycleRiskDecisionLabel,
    pub outcome: CycleOutcome,
    pub created_at_ms: i64,
}

impl CycleTraceSummary {
    #[must_use]
    pub fn from_identity(
        identity: &TraceIdentity,
        signal: IndicatorSignal,
        intent: TradeIntent,
        risk_decision: CycleRiskDecisionLabel,
        outcome: CycleOutcome,
        created_at_ms: i64,
    ) -> Self {
        Self {
            trace_id: identity.trace_id.clone(),
            bot_id: identity.bot_id.clone(),
            symbol: identity.symbol.clone(),
            bar_timestamp: identity.bar_timestamp.clone(),
            phase: identity.phase,
            trigger_kind: identity.trigger_kind,
            signal,
            intent,
            risk_decision,
            outcome,
            created_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CycleTriggerKind;
    use openticker_core::{IndicatorSignal, SignalPhase, TradeIntent};

    #[test]
    fn summary_copies_trace_identity_fields() {
        let identity = TraceIdentity::with_trace_id(
            "trc_123",
            "aapl",
            "AAPL",
            "2026-01-01T00:00:00Z",
            SignalPhase::Confirmed,
            CycleTriggerKind::MarketBar,
        );

        let summary = CycleTraceSummary::from_identity(
            &identity,
            IndicatorSignal::BuyConfirmed,
            TradeIntent::OpenLong,
            CycleRiskDecisionLabel::Allowed,
            CycleOutcome::AcceptedFilled,
            42,
        );

        assert_eq!(summary.trace_id, "trc_123");
        assert_eq!(summary.bot_id, "aapl");
        assert_eq!(summary.symbol, "AAPL");
        assert_eq!(summary.phase, SignalPhase::Confirmed);
        assert_eq!(summary.trigger_kind, CycleTriggerKind::MarketBar);
    }
}
