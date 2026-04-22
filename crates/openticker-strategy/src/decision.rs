use openticker_core::TradeIntent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyDecision {
    pub intent: TradeIntent,
    pub rationale: Option<String>,
}

impl StrategyDecision {
    #[must_use]
    pub fn new(intent: TradeIntent, rationale: impl Into<String>) -> Self {
        Self {
            intent,
            rationale: Some(rationale.into()),
        }
    }

    #[must_use]
    pub fn no_op(rationale: impl Into<String>) -> Self {
        Self::new(TradeIntent::NoOp, rationale)
    }
}
