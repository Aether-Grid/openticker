use crate::{
    CapitalState, CycleTraceSummary, CycleTrigger, ExecutionStep, IntentStep, PositionStep,
    ReconciliationContext, RelatedEvent, RelatedRecord, RiskStep, SignalStep,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleTrace {
    pub summary: CycleTraceSummary,
    pub trigger: CycleTrigger,
    pub signal_step: SignalStep,
    pub intent_step: IntentStep,
    pub risk_step: RiskStep,
    pub execution_step: ExecutionStep,
    pub position_step: PositionStep,
    pub capital_before: CapitalState,
    pub capital_after: CapitalState,
    pub reconciliation_context: ReconciliationContext,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_records: Vec<RelatedRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_events: Vec<RelatedEvent>,
}
