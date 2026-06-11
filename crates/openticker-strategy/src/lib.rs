mod consensus;
mod context;
mod decision;
mod metadata;
mod single_indicator;
mod traits;

pub use consensus::{ConsensusConfigError, ConsensusLongOnlyStrategy};
pub use context::{ConsensusStrategyContext, IndicatorObservation, StrategyContext};
pub use decision::StrategyDecision;
pub use single_indicator::SingleIndicatorLongOnlyStrategy;
pub use traits::{ConsensusStrategy, Strategy};

#[cfg(kani)]
mod proofs;

#[cfg(test)]
mod tests;
