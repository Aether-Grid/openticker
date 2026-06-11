use crate::common::{Sma, crossover, crossunder, indicator_param_usize};
use crate::{
    IndicatorBuildError, IndicatorCapabilities, IndicatorDescriptor, IndicatorEngine,
    IndicatorEvaluation, IndicatorManifest, IndicatorMarketSupport, IndicatorMetadataCapabilities,
    IndicatorRole, IndicatorSignal, IndicatorStabilityClass, IndicatorWarmupRequirements,
    SignalSnapshot,
};
use openticker_core::{IndicatorFactValue, OhlcvBar, SignalMetadata, SignalPhase};
use std::collections::BTreeMap;
use thiserror::Error;
use toml::Table;

const DEFAULT_FAST_LENGTH: usize = 10;
const DEFAULT_SLOW_LENGTH: usize = 30;
/// Upper bound on SMA window lengths. A window backs a `VecDeque` allocation, so
/// an unbounded length (e.g. tens of millions) would attempt a large allocation
/// for no practical trading benefit. 10,000 bars comfortably exceeds any sane
/// configuration while keeping memory bounded.
const MAX_LENGTH: usize = 10_000;
const ALLOWED_ROLES: &[IndicatorRole] = &[IndicatorRole::PrimarySignal];

const CAPABILITIES: IndicatorCapabilities = IndicatorCapabilities {
    supports_intrabar: true,
    supports_preview: true,
    supports_confirmed: true,
};

const WARMUP: IndicatorWarmupRequirements = IndicatorWarmupRequirements {
    minimum_confirmed_bars: 50,
    recommended_backfill_bars: 200,
};

const METADATA: IndicatorMetadataCapabilities = IndicatorMetadataCapabilities {
    supports_strength: false,
    supports_reason_code: true,
    supports_tags: false,
    supports_facts: true,
    supports_trade_levels: false,
};

pub const MANIFEST: IndicatorManifest = IndicatorManifest {
    type_id: "sma_crossover",
    family: "trend",
    role_default: IndicatorRole::PrimarySignal,
    allowed_roles: ALLOWED_ROLES,
    stability_class: IndicatorStabilityClass::StableOnClose,
    market_support: IndicatorMarketSupport::BOTH,
    capabilities: CAPABILITIES,
    warmup: WARMUP,
    metadata: METADATA,
};

pub const DESCRIPTOR: IndicatorDescriptor = IndicatorDescriptor {
    manifest: &MANIFEST,
    build,
};

/// Validated parameters for the SMA-crossover indicator.
///
/// The fields are private so the only construction paths are the validating
/// [`SmaCrossoverParams::new`] / [`SmaCrossoverIndicator::try_new`] constructors
/// and [`SmaCrossoverParams::default`]. This prevents bypassing validation (the
/// non-zero, upper-bound, and `fast_length < slow_length` checks) via a struct
/// literal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmaCrossoverParams {
    fast_length: usize,
    slow_length: usize,
}

impl SmaCrossoverParams {
    /// # Errors
    ///
    /// Returns [`SmaCrossoverError`] when one or more parameter values are
    /// invalid (zero length, length above the supported maximum, or
    /// `fast_length >= slow_length`).
    pub fn new(fast_length: usize, slow_length: usize) -> Result<Self, SmaCrossoverError> {
        if fast_length == 0 {
            return Err(SmaCrossoverError::InvalidFastLength(fast_length));
        }
        if slow_length == 0 {
            return Err(SmaCrossoverError::InvalidSlowLength(slow_length));
        }
        if fast_length > MAX_LENGTH {
            return Err(SmaCrossoverError::FastLengthTooLarge {
                fast_length,
                max: MAX_LENGTH,
            });
        }
        if slow_length > MAX_LENGTH {
            return Err(SmaCrossoverError::SlowLengthTooLarge {
                slow_length,
                max: MAX_LENGTH,
            });
        }
        if fast_length >= slow_length {
            return Err(SmaCrossoverError::InvalidWindowOrder {
                fast_length,
                slow_length,
            });
        }
        Ok(Self {
            fast_length,
            slow_length,
        })
    }
}

impl Default for SmaCrossoverParams {
    fn default() -> Self {
        Self {
            fast_length: DEFAULT_FAST_LENGTH,
            slow_length: DEFAULT_SLOW_LENGTH,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmaCrossoverSnapshot {
    pub fast_sma: Option<f64>,
    pub slow_sma: Option<f64>,
    pub fast_length: usize,
    pub slow_length: usize,
    pub signal: IndicatorSignal,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SmaCrossoverError {
    #[error("fast_length must be greater than zero, got `{0}`")]
    InvalidFastLength(usize),
    #[error("slow_length must be greater than zero, got `{0}`")]
    InvalidSlowLength(usize),
    #[error("fast_length `{fast_length}` must not exceed `{max}`")]
    FastLengthTooLarge { fast_length: usize, max: usize },
    #[error("slow_length `{slow_length}` must not exceed `{max}`")]
    SlowLengthTooLarge { slow_length: usize, max: usize },
    #[error("fast_length `{fast_length}` must be less than slow_length `{slow_length}`")]
    InvalidWindowOrder {
        fast_length: usize,
        slow_length: usize,
    },
}

#[derive(Debug, Clone)]
pub struct SmaCrossoverIndicator {
    params: SmaCrossoverParams,
    fast_sma: Sma,
    slow_sma: Sma,
    prev_fast_sma: Option<f64>,
    prev_slow_sma: Option<f64>,
    /// Timestamp of the most recently processed bar, used by
    /// [`SmaCrossoverIndicator::is_bar_out_of_order`].
    ///
    /// This field is an opt-in ordering guard for callers that cannot guarantee
    /// monotonic bar delivery. The indicator does not self-enforce ordering;
    /// equal timestamps are treated as in-order to support two-phase
    /// Preview/Confirmed re-evaluation of the same bar. Stored as epoch
    /// milliseconds to avoid pulling a date-time type into the indicator state.
    last_bar_timestamp_ms: Option<i64>,
}

impl SmaCrossoverIndicator {
    #[must_use]
    pub fn new(params: SmaCrossoverParams) -> Self {
        Self {
            fast_sma: Sma::new(params.fast_length),
            slow_sma: Sma::new(params.slow_length),
            params,
            prev_fast_sma: None,
            prev_slow_sma: None,
            last_bar_timestamp_ms: None,
        }
    }

    /// # Errors
    ///
    /// Returns [`SmaCrossoverError`] when one or more parameter values are invalid.
    pub fn try_new(fast_length: usize, slow_length: usize) -> Result<Self, SmaCrossoverError> {
        SmaCrossoverParams::new(fast_length, slow_length).map(Self::new)
    }

    /// Returns `true` when `bar` would be applied out of order relative to the
    /// most recently processed bar.
    ///
    /// This is an opt-in ordering check for callers that cannot guarantee
    /// monotonic bar delivery; the indicator does not self-enforce ordering.
    /// Equal timestamps return `false` because the same bar is legitimately
    /// re-evaluated across the `Preview` and `Confirmed` phases.
    #[must_use]
    pub fn is_bar_out_of_order(&self, bar: &OhlcvBar) -> bool {
        self.last_bar_timestamp_ms
            .is_some_and(|previous_ms| bar.timestamp.timestamp_millis() < previous_ms)
    }

    /// Updates the indicator with a new bar and returns the resulting snapshot.
    ///
    /// # Sequential-update contract
    ///
    /// Bars **must** be supplied in non-decreasing timestamp order. Crossover
    /// detection relies on `prev_*` state that this method mutates as a side
    /// effect, so a bar whose timestamp moves *backwards* would be compared
    /// against stale previous values and could produce a spurious or incorrect
    /// signal. Equal timestamps are permitted: the same bar is intentionally
    /// evaluated twice, once in the `Preview` phase and once in the `Confirmed`
    /// phase.
    ///
    /// Ordering is owned by the caller (the runtime/lane layer, which dedups and
    /// orders bars via its own last-dispatched-timestamp guards, and which
    /// replays historical bars during warmup/recovery). Rather than panic on a
    /// regression — which would conflict with those legitimate replay flows —
    /// this indicator records the last processed timestamp and exposes
    /// [`SmaCrossoverIndicator::is_bar_out_of_order`] so a caller can detect a
    /// stale update before applying it.
    #[must_use]
    pub fn update(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> SmaCrossoverSnapshot {
        let bar_timestamp_ms = bar.timestamp.timestamp_millis();

        let fast_sma = self.fast_sma.update(bar.close);
        let slow_sma = self.slow_sma.update(bar.close);

        let signal = match (fast_sma, slow_sma) {
            (Some(fast), Some(slow))
                if crossover(self.prev_fast_sma, self.prev_slow_sma, fast, slow) =>
            {
                match phase {
                    SignalPhase::Preview => IndicatorSignal::BuyPreview,
                    SignalPhase::Confirmed => IndicatorSignal::BuyConfirmed,
                }
            }
            (Some(fast), Some(slow))
                if crossunder(self.prev_fast_sma, self.prev_slow_sma, fast, slow) =>
            {
                match phase {
                    SignalPhase::Preview => IndicatorSignal::SellPreview,
                    SignalPhase::Confirmed => IndicatorSignal::SellConfirmed,
                }
            }
            _ => IndicatorSignal::None,
        };

        self.prev_fast_sma = fast_sma;
        self.prev_slow_sma = slow_sma;
        self.last_bar_timestamp_ms = Some(bar_timestamp_ms);

        SmaCrossoverSnapshot {
            fast_sma,
            slow_sma,
            fast_length: self.params.fast_length,
            slow_length: self.params.slow_length,
            signal,
        }
    }
}

impl Default for SmaCrossoverIndicator {
    fn default() -> Self {
        Self::new(SmaCrossoverParams::default())
    }
}

impl IndicatorEngine for SmaCrossoverIndicator {
    fn type_id(&self) -> &'static str {
        MANIFEST.type_id
    }

    fn evaluate(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> IndicatorEvaluation {
        IndicatorEvaluation::from_snapshot(&self.update(bar, phase))
    }

    fn clone_engine(&self) -> Box<dyn IndicatorEngine> {
        Box::new(self.clone())
    }
}

impl SignalSnapshot for SmaCrossoverSnapshot {
    fn signal(&self) -> IndicatorSignal {
        self.signal
    }

    fn metadata(&self) -> SignalMetadata {
        let mut facts = BTreeMap::new();
        if let Some(fast_sma) = self.fast_sma {
            facts.insert("fast_sma".to_owned(), IndicatorFactValue::from(fast_sma));
        }
        if let Some(slow_sma) = self.slow_sma {
            facts.insert("slow_sma".to_owned(), IndicatorFactValue::from(slow_sma));
        }
        facts.insert(
            "fast_length".to_owned(),
            IndicatorFactValue::from(self.fast_length),
        );
        facts.insert(
            "slow_length".to_owned(),
            IndicatorFactValue::from(self.slow_length),
        );

        let reason_code = match self.signal {
            IndicatorSignal::BuyPreview | IndicatorSignal::BuyConfirmed => Some("sma_cross_up"),
            IndicatorSignal::SellPreview | IndicatorSignal::SellConfirmed => Some("sma_cross_down"),
            IndicatorSignal::None => None,
        };

        SignalMetadata {
            strength: None,
            reason_code: reason_code.map(str::to_owned),
            tags: Vec::new(),
            facts,
            trade_levels: None,
        }
    }
}

fn build(params: &Table) -> Result<Box<dyn IndicatorEngine>, IndicatorBuildError> {
    let fast_length = indicator_param_usize(params, "fast_length").unwrap_or(DEFAULT_FAST_LENGTH);
    let slow_length = indicator_param_usize(params, "slow_length").unwrap_or(DEFAULT_SLOW_LENGTH);
    SmaCrossoverIndicator::try_new(fast_length, slow_length)
        .map(|indicator| Box::new(indicator) as Box<dyn IndicatorEngine>)
        .map_err(|error| IndicatorBuildError::InvalidParameters(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn bar(timestamp: &str, close: f64) -> OhlcvBar {
        OhlcvBar {
            timestamp: DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
            open: close,
            high: close + 0.4,
            low: close - 0.4,
            close,
            volume: 100.0,
        }
    }

    #[test]
    fn validates_window_params() {
        assert!(matches!(
            SmaCrossoverParams::new(10, 10),
            Err(SmaCrossoverError::InvalidWindowOrder { .. })
        ));
    }

    #[test]
    fn replay_is_deterministic() {
        let bars = [
            bar("2026-01-01T00:00:00Z", 100.0),
            bar("2026-01-01T00:01:00Z", 99.0),
            bar("2026-01-01T00:02:00Z", 98.0),
            bar("2026-01-01T00:03:00Z", 101.0),
            bar("2026-01-01T00:04:00Z", 102.0),
            bar("2026-01-01T00:05:00Z", 103.0),
        ];

        let params = SmaCrossoverParams::new(2, 3).unwrap();
        let mut a = SmaCrossoverIndicator::new(params);
        let mut b = SmaCrossoverIndicator::new(params);

        let out_a = bars
            .iter()
            .map(|bar| a.update(bar, SignalPhase::Confirmed))
            .collect::<Vec<_>>();
        let out_b = bars
            .iter()
            .map(|bar| b.update(bar, SignalPhase::Confirmed))
            .collect::<Vec<_>>();

        assert_eq!(out_a, out_b);
    }

    #[test]
    fn rejects_over_large_lengths() {
        assert!(matches!(
            SmaCrossoverParams::new(MAX_LENGTH + 1, MAX_LENGTH + 2),
            Err(SmaCrossoverError::FastLengthTooLarge { .. })
        ));
        assert!(matches!(
            SmaCrossoverParams::new(10, MAX_LENGTH + 1),
            Err(SmaCrossoverError::SlowLengthTooLarge { .. })
        ));
        // The boundary value is accepted.
        assert!(SmaCrossoverParams::new(MAX_LENGTH - 1, MAX_LENGTH).is_ok());
    }

    #[test]
    fn equal_previous_smas_then_move_up_is_not_a_crossover() {
        // Direct check of the crossover semantics: when the fast and slow SMAs
        // were exactly equal on the previous bar, a subsequent upward move is a
        // flat-period breakout, not a crossing, and must not fire.
        assert!(!crossover(Some(50.0), Some(50.0), 51.0, 50.5));
        // Strictly-below previous values followed by a move above is a genuine
        // crossover and still fires.
        assert!(crossover(Some(49.0), Some(50.0), 51.0, 50.5));
    }

    #[test]
    fn equal_previous_smas_then_move_down_is_not_a_crossunder() {
        assert!(!crossunder(Some(50.0), Some(50.0), 49.0, 49.5));
        assert!(crossunder(Some(51.0), Some(50.0), 49.0, 49.5));
    }

    #[test]
    fn flat_then_breakout_produces_no_signal_through_indicator() {
        // Feed a flat run so the fast and slow SMAs converge to the same value,
        // then break upward. Under the corrected (strict) crossover semantics no
        // buy signal should be emitted from the flat-to-breakout transition.
        let params = SmaCrossoverParams::new(2, 3).unwrap();
        let mut indicator = SmaCrossoverIndicator::new(params);
        let flat_then_up = [
            bar("2026-01-01T00:00:00Z", 100.0),
            bar("2026-01-01T00:01:00Z", 100.0),
            bar("2026-01-01T00:02:00Z", 100.0),
            bar("2026-01-01T00:03:00Z", 100.0),
            bar("2026-01-01T00:04:00Z", 100.0),
            bar("2026-01-01T00:05:00Z", 101.0),
        ];
        let signals = flat_then_up
            .iter()
            .map(|bar| indicator.update(bar, SignalPhase::Confirmed).signal)
            .collect::<Vec<_>>();
        // While prices were flat, fast and slow SMAs are equal, so the upward
        // move must not be reported as a crossover.
        assert!(
            signals.iter().all(|signal| !matches!(
                signal,
                IndicatorSignal::BuyPreview | IndicatorSignal::BuyConfirmed
            )),
            "flat-to-breakout should not emit a buy signal, got {signals:?}"
        );
    }

    #[test]
    fn out_of_order_detection_flags_backwards_bars_only() {
        let params = SmaCrossoverParams::new(2, 3).unwrap();
        let mut indicator = SmaCrossoverIndicator::new(params);

        let first = bar("2026-01-01T00:05:00Z", 100.0);
        // Before any update there is no reference timestamp, so nothing is stale.
        assert!(!indicator.is_bar_out_of_order(&first));
        let _ = indicator.update(&first, SignalPhase::Confirmed);

        // An equal timestamp (Preview/Confirmed re-evaluation) is in order.
        assert!(!indicator.is_bar_out_of_order(&first));
        // A strictly earlier timestamp is an out-of-order regression.
        let earlier = bar("2026-01-01T00:04:00Z", 101.0);
        assert!(indicator.is_bar_out_of_order(&earlier));
        // A later timestamp is in order.
        let later = bar("2026-01-01T00:06:00Z", 102.0);
        assert!(!indicator.is_bar_out_of_order(&later));
    }

    #[test]
    fn equal_timestamps_are_allowed_for_two_phase_evaluation() {
        // The same bar is intentionally evaluated in Preview then Confirmed; that
        // re-evaluation reuses the same timestamp and is not treated as stale.
        let params = SmaCrossoverParams::new(2, 3).unwrap();
        let mut indicator = SmaCrossoverIndicator::new(params);
        let same_bar = bar("2026-01-01T00:00:00Z", 100.0);
        let _ = indicator.update(&same_bar, SignalPhase::Preview);
        assert!(!indicator.is_bar_out_of_order(&same_bar));
        let _ = indicator.update(&same_bar, SignalPhase::Confirmed);
        let later = bar("2026-01-01T00:01:00Z", 101.0);
        let _ = indicator.update(&later, SignalPhase::Confirmed);
    }
}
