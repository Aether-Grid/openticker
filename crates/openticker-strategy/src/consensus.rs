use openticker_core::{IndicatorRole, IndicatorSignal, IndicatorSignalPolicy, TradeIntent};
use thiserror::Error;

use crate::{
    context::{ConsensusStrategyContext, IndicatorObservation},
    decision::StrategyDecision,
    metadata::metadata_filter_block_reason,
    traits::ConsensusStrategy,
};

/// Error returned by [`ConsensusLongOnlyStrategy::new`] when threshold
/// validation fails.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ConsensusConfigError {
    /// The named threshold was negative or not finite.
    ///
    /// Both thresholds gate a weighted score comparison; a negative value
    /// inverts the comparison direction and a non-finite value produces
    /// undefined comparisons, so neither is accepted.
    #[error("`{field}` must be finite and non-negative")]
    InvalidThreshold {
        /// Name of the offending field (`"entry_threshold"` or
        /// `"exit_threshold"`).
        field: &'static str,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ConsensusLongOnlyStrategy {
    /// Minimum weighted primary score required to take a long entry.
    ///
    /// Invariant: must be finite and non-negative. The decision logic compares
    /// `primary_score >= entry_threshold`; a negative threshold would let a
    /// negative (net-bearish) score open a long, which is nonsensical. Prefer
    /// constructing via [`ConsensusLongOnlyStrategy::new`], which enforces this.
    pub entry_threshold: f64,
    /// Minimum magnitude of a negative weighted primary score required to exit
    /// (close) a long.
    ///
    /// Invariant: must be finite and non-negative. The decision logic compares
    /// `primary_score <= -exit_threshold`; a negative threshold would invert the
    /// comparison and exit on a positive (net-bullish) score. Prefer
    /// constructing via [`ConsensusLongOnlyStrategy::new`], which enforces this.
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
    /// Creates a consensus strategy with validated thresholds.
    ///
    /// Both thresholds gate the weighted primary score: an entry requires
    /// `primary_score >= entry_threshold` and an exit requires
    /// `primary_score <= -exit_threshold`. Negative or non-finite thresholds
    /// make those comparisons behave unintuitively (e.g. a negative
    /// `entry_threshold` would let a net-bearish score open a long), so they are
    /// rejected here.
    ///
    /// # Errors
    ///
    /// Returns [`ConsensusConfigError`] when either threshold is negative or not
    /// finite.
    pub fn new(entry_threshold: f64, exit_threshold: f64) -> Result<Self, ConsensusConfigError> {
        if !entry_threshold.is_finite() || entry_threshold < 0.0 {
            return Err(ConsensusConfigError::InvalidThreshold {
                field: "entry_threshold",
            });
        }
        if !exit_threshold.is_finite() || exit_threshold < 0.0 {
            return Err(ConsensusConfigError::InvalidThreshold {
                field: "exit_threshold",
            });
        }
        Ok(Self {
            entry_threshold,
            exit_threshold,
        })
    }

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
                // Filters are only evaluated for a non-zero direction: the
                // `direction == 0` (no-trade) case returns earlier above, so this
                // branch is reached only when `direction == -1` (a sell/close). The
                // `else` arm below therefore vetoes a sell when a filter votes long
                // (`vote > 0.0`); it never runs for `direction == 0`, even though the
                // `vote > 0.0` comparison might read as if it could veto a no-trade
                // decision.
                if direction > 0 {
                    // Entry (long): a filter voting short (`vote < 0.0`) vetoes.
                    vote < 0.0
                } else {
                    // Exit (`direction == -1`): a filter voting long (`vote > 0.0`) vetoes.
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
