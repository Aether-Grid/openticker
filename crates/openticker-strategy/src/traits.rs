use crate::{
    context::{ConsensusStrategyContext, StrategyContext},
    decision::StrategyDecision,
};

pub trait Strategy {
    fn decide(&mut self, context: StrategyContext<'_>) -> StrategyDecision;
}

pub trait ConsensusStrategy {
    fn decide_consensus(&mut self, context: ConsensusStrategyContext<'_>) -> StrategyDecision;
}
