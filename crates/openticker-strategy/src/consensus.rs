use openticker_core::{IndicatorRole, IndicatorSignal, IndicatorSignalPolicy, TradeIntent};

use crate::{
    context::{ConsensusStrategyContext, IndicatorObservation},
    decision::StrategyDecision,
    metadata::metadata_filter_block_reason,
    traits::ConsensusStrategy,
};

#[derive(Debug, Clone, Copy)]
pub struct ConsensusLongOnlyStrategy {
    pub entry_threshold: f64,
    pub exit_threshold: f64,
}

impl Default for ConsensusLongOnlyStrategy {
    fn default() -> Self {
        Self {
            entry_threshold: 1.0,
            exit_threshold: 1.0,
        }
    }
}

impl ConsensusLongOnlyStrategy {
    fn effective_signal(observation: &IndicatorObservation<'_>) -> IndicatorSignal {
        match (observation.signal_policy, observation.signal) {
            (
                IndicatorSignalPolicy::ConfirmedRequired,
                IndicatorSignal::BuyPreview | IndicatorSignal::SellPreview,
            ) => IndicatorSignal::None,
            (_, signal) => signal,
        }
    }

    fn signal_vote(policy: IndicatorSignalPolicy, signal: IndicatorSignal) -> f64 {
        match signal {
            IndicatorSignal::BuyConfirmed => 1.0,
            IndicatorSignal::SellConfirmed => -1.0,
            IndicatorSignal::BuyPreview => {
                if policy == IndicatorSignalPolicy::PreviewAllowed {
                    1.0
                } else {
                    0.0
                }
            }
            IndicatorSignal::SellPreview => {
                if policy == IndicatorSignalPolicy::PreviewAllowed {
                    -1.0
                } else {
                    0.0
                }
            }
            IndicatorSignal::None => 0.0,
        }
    }

    fn intent_for_direction(has_position: bool, direction: i8) -> TradeIntent {
        match direction {
            1 => {
                if has_position {
                    TradeIntent::AddLong
                } else {
                    TradeIntent::OpenLong
                }
            }
            -1 => {
                if has_position {
                    TradeIntent::CloseLong
                } else {
                    TradeIntent::NoOp
                }
            }
            _ => TradeIntent::NoOp,
        }
    }
}

impl ConsensusStrategy for ConsensusLongOnlyStrategy {
    fn decide_consensus(&mut self, context: ConsensusStrategyContext<'_>) -> StrategyDecision {
        let mut metadata_filtered = Vec::new();

        let primary_score = context
            .indicators
            .iter()
            .filter(|obs| obs.role == IndicatorRole::PrimarySignal)
            .fold(0.0, |acc, obs| {
                let effective_signal = Self::effective_signal(obs);
                if let Some(reason) = metadata_filter_block_reason(
                    obs.metadata_capabilities,
                    obs.metadata_filters,
                    obs.metadata,
                    effective_signal,
                ) {
                    metadata_filtered.push(format!("{}:{reason}", obs.id));
                    acc
                } else {
                    acc + obs.weight * Self::signal_vote(obs.signal_policy, effective_signal)
                }
            });

        let has_primary = context
            .indicators
            .iter()
            .any(|obs| obs.role == IndicatorRole::PrimarySignal);
        if !has_primary {
            return StrategyDecision::no_op("no_primary_indicator");
        }

        let direction = if primary_score >= self.entry_threshold {
            1
        } else if primary_score <= -self.exit_threshold {
            -1
        } else {
            0
        };
        if direction == 0 {
            let rationale = if metadata_filtered.is_empty() {
                format!(
                    "primary_score={primary_score},entry_threshold={},exit_threshold={}",
                    self.entry_threshold, self.exit_threshold
                )
            } else {
                format!(
                    "primary_score={primary_score},entry_threshold={},exit_threshold={},metadata_filtered={}",
                    self.entry_threshold,
                    self.exit_threshold,
                    metadata_filtered.join(";")
                )
            };
            return StrategyDecision::no_op(rationale);
        }

        let filter_veto = context
            .indicators
            .iter()
            .filter(|obs| obs.role == IndicatorRole::Filter)
            .any(|obs| {
                let effective_signal = Self::effective_signal(obs);
                if let Some(reason) = metadata_filter_block_reason(
                    obs.metadata_capabilities,
                    obs.metadata_filters,
                    obs.metadata,
                    effective_signal,
                ) {
                    metadata_filtered.push(format!("{}:{reason}", obs.id));
                    return false;
                }

                let vote = Self::signal_vote(obs.signal_policy, effective_signal);
                if direction > 0 {
                    vote < 0.0
                } else {
                    vote > 0.0
                }
            });

        if filter_veto {
            let rationale = if metadata_filtered.is_empty() {
                "filter_veto".to_owned()
            } else {
                format!(
                    "filter_veto,metadata_filtered={}",
                    metadata_filtered.join(";")
                )
            };
            StrategyDecision::no_op(rationale)
        } else {
            let intent = Self::intent_for_direction(context.has_position, direction);
            let rationale = if metadata_filtered.is_empty() {
                format!(
                    "primary_score={primary_score},direction={direction},entry_threshold={},exit_threshold={}",
                    self.entry_threshold, self.exit_threshold
                )
            } else {
                format!(
                    "primary_score={primary_score},direction={direction},entry_threshold={},exit_threshold={},metadata_filtered={}",
                    self.entry_threshold,
                    self.exit_threshold,
                    metadata_filtered.join(";")
                )
            };
            StrategyDecision::new(intent, rationale)
        }
    }
}
