use crate::common::{Rsi, crossover, crossunder, indicator_param_f64, indicator_param_usize};
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

const DEFAULT_PERIOD: usize = 14;
const DEFAULT_OVERSOLD: f64 = 30.0;
const DEFAULT_OVERBOUGHT: f64 = 70.0;
/// Upper bound on the RSI period. The period seeds the Wilder RMA averaging
/// window; an unbounded period serves no practical purpose and risks large
/// allocations / lossy conversions. 10,000 bars exceeds any sane configuration.
const MAX_PERIOD: usize = 10_000;
const ALLOWED_ROLES: &[IndicatorRole] = &[IndicatorRole::Filter, IndicatorRole::Context];

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
    type_id: "rsi_threshold",
    family: "momentum",
    role_default: IndicatorRole::Filter,
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

/// Validated parameters for the RSI-threshold indicator.
///
/// The fields are private so the only construction paths are the validating
/// [`RsiThresholdParams::new`] / [`RsiThresholdIndicator::try_new`] constructors
/// and [`RsiThresholdParams::default`]. This prevents bypassing validation (in
/// particular the `oversold < overbought` ordering check) via a struct literal,
/// which previously created a dead zone where the RSI could never cross either
/// threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RsiThresholdParams {
    period: usize,
    oversold: f64,
    overbought: f64,
}

impl RsiThresholdParams {
    /// # Errors
    ///
    /// Returns [`RsiThresholdError`] when one or more parameter values are
    /// invalid (zero period, period above the supported maximum, thresholds
    /// outside `0..100`, or `oversold >= overbought`).
    pub fn new(period: usize, oversold: f64, overbought: f64) -> Result<Self, RsiThresholdError> {
        if period == 0 {
            return Err(RsiThresholdError::InvalidPeriod(period));
        }
        if period > MAX_PERIOD {
            return Err(RsiThresholdError::PeriodTooLarge {
                period,
                max: MAX_PERIOD,
            });
        }
        if !(0.0..100.0).contains(&oversold) {
            return Err(RsiThresholdError::InvalidOversold(oversold));
        }
        if !(0.0..100.0).contains(&overbought) {
            return Err(RsiThresholdError::InvalidOverbought(overbought));
        }
        if oversold >= overbought {
            return Err(RsiThresholdError::InvalidThresholdOrder {
                oversold,
                overbought,
            });
        }
        Ok(Self {
            period,
            oversold,
            overbought,
        })
    }
}

impl Default for RsiThresholdParams {
    fn default() -> Self {
        Self {
            period: DEFAULT_PERIOD,
            oversold: DEFAULT_OVERSOLD,
            overbought: DEFAULT_OVERBOUGHT,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RsiThresholdSnapshot {
    pub rsi: Option<f64>,
    pub period: usize,
    pub oversold: f64,
    pub overbought: f64,
    pub signal: IndicatorSignal,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum RsiThresholdError {
    #[error("period must be greater than zero, got `{0}`")]
    InvalidPeriod(usize),
    #[error("period `{period}` must not exceed `{max}`")]
    PeriodTooLarge { period: usize, max: usize },
    #[error("oversold must be between 0 and 100, got `{0}`")]
    InvalidOversold(f64),
    #[error("overbought must be between 0 and 100, got `{0}`")]
    InvalidOverbought(f64),
    #[error("oversold `{oversold}` must be less than overbought `{overbought}`")]
    InvalidThresholdOrder { oversold: f64, overbought: f64 },
}

#[derive(Debug, Clone)]
pub struct RsiThresholdIndicator {
    params: RsiThresholdParams,
    rsi: Rsi,
    prev_rsi: Option<f64>,
    /// Timestamp of the most recently processed bar, used by
    /// [`RsiThresholdIndicator::is_bar_out_of_order`].
    ///
    /// This field is an opt-in ordering guard for callers that cannot guarantee
    /// monotonic bar delivery. The indicator does not self-enforce ordering;
    /// equal timestamps are treated as in-order to support two-phase
    /// Preview/Confirmed re-evaluation of the same bar. Stored as epoch
    /// milliseconds to avoid pulling a date-time type into the indicator state.
    last_bar_timestamp_ms: Option<i64>,
}

impl RsiThresholdIndicator {
    #[must_use]
    pub fn new(params: RsiThresholdParams) -> Self {
        Self {
            rsi: Rsi::new(params.period),
            params,
            prev_rsi: None,
            last_bar_timestamp_ms: None,
        }
    }

    /// # Errors
    ///
    /// Returns [`RsiThresholdError`] when one or more parameter values are invalid.
    pub fn try_new(
        period: usize,
        oversold: f64,
        overbought: f64,
    ) -> Result<Self, RsiThresholdError> {
        RsiThresholdParams::new(period, oversold, overbought).map(Self::new)
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
    /// Bars **must** be supplied in non-decreasing timestamp order.
    /// Threshold-crossing detection relies on `prev_rsi`, which this method
    /// mutates as a side effect, so a bar whose timestamp moves *backwards* would
    /// be compared against stale state and could produce a spurious or incorrect
    /// signal. Equal timestamps are permitted for the Preview/Confirmed two-phase
    /// re-evaluation of the same bar.
    ///
    /// Ordering is owned by the caller (the runtime/lane layer, which dedups and
    /// orders bars and replays historical bars during warmup/recovery). Rather
    /// than panic on a regression, this indicator records the last processed
    /// timestamp and exposes [`RsiThresholdIndicator::is_bar_out_of_order`] so a
    /// caller can detect a stale update before applying it.
    #[must_use]
    pub fn update(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> RsiThresholdSnapshot {
        let bar_timestamp_ms = bar.timestamp.timestamp_millis();

        let rsi = self.rsi.update(bar.close);
        let signal = match rsi {
            Some(value)
                if crossover(
                    self.prev_rsi,
                    Some(self.params.oversold),
                    value,
                    self.params.oversold,
                ) =>
            {
                match phase {
                    SignalPhase::Preview => IndicatorSignal::BuyPreview,
                    SignalPhase::Confirmed => IndicatorSignal::BuyConfirmed,
                }
            }
            Some(value)
                if crossunder(
                    self.prev_rsi,
                    Some(self.params.overbought),
                    value,
                    self.params.overbought,
                ) =>
            {
                match phase {
                    SignalPhase::Preview => IndicatorSignal::SellPreview,
                    SignalPhase::Confirmed => IndicatorSignal::SellConfirmed,
                }
            }
            _ => IndicatorSignal::None,
        };

        self.prev_rsi = rsi;
        self.last_bar_timestamp_ms = Some(bar_timestamp_ms);

        RsiThresholdSnapshot {
            rsi,
            period: self.params.period,
            oversold: self.params.oversold,
            overbought: self.params.overbought,
            signal,
        }
    }
}

impl Default for RsiThresholdIndicator {
    fn default() -> Self {
        Self::new(RsiThresholdParams::default())
    }
}

impl IndicatorEngine for RsiThresholdIndicator {
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

impl SignalSnapshot for RsiThresholdSnapshot {
    fn signal(&self) -> IndicatorSignal {
        self.signal
    }

    fn metadata(&self) -> SignalMetadata {
        let mut facts = BTreeMap::new();
        if let Some(rsi) = self.rsi {
            facts.insert("rsi".to_owned(), IndicatorFactValue::from(rsi));
        }
        facts.insert("period".to_owned(), IndicatorFactValue::from(self.period));
        facts.insert(
            "oversold".to_owned(),
            IndicatorFactValue::from(self.oversold),
        );
        facts.insert(
            "overbought".to_owned(),
            IndicatorFactValue::from(self.overbought),
        );

        let reason_code = match self.signal {
            IndicatorSignal::BuyPreview | IndicatorSignal::BuyConfirmed => {
                Some("rsi_recovery_from_oversold")
            }
            IndicatorSignal::SellPreview | IndicatorSignal::SellConfirmed => {
                Some("rsi_drop_from_overbought")
            }
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
    let period = indicator_param_usize(params, "period").unwrap_or(DEFAULT_PERIOD);
    let oversold = indicator_param_f64(params, "oversold").unwrap_or(DEFAULT_OVERSOLD);
    let overbought = indicator_param_f64(params, "overbought").unwrap_or(DEFAULT_OVERBOUGHT);
    RsiThresholdIndicator::try_new(period, oversold, overbought)
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
            high: close + 0.2,
            low: close - 0.2,
            close,
            volume: 50.0,
        }
    }

    #[test]
    fn validates_params() {
        assert!(matches!(
            RsiThresholdParams::new(0, 30.0, 70.0),
            Err(RsiThresholdError::InvalidPeriod(0))
        ));
    }

    #[test]
    fn replay_is_deterministic() {
        let mut a = RsiThresholdIndicator::default();
        let mut b = RsiThresholdIndicator::default();
        let bars = [
            bar("2026-01-01T00:00:00Z", 100.0),
            bar("2026-01-01T00:01:00Z", 99.0),
            bar("2026-01-01T00:02:00Z", 98.0),
            bar("2026-01-01T00:03:00Z", 97.0),
            bar("2026-01-01T00:04:00Z", 99.5),
            bar("2026-01-01T00:05:00Z", 101.0),
        ];

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
    fn rejects_over_large_period() {
        assert!(matches!(
            RsiThresholdParams::new(MAX_PERIOD + 1, 30.0, 70.0),
            Err(RsiThresholdError::PeriodTooLarge { .. })
        ));
        // The boundary value is accepted.
        assert!(RsiThresholdParams::new(MAX_PERIOD, 30.0, 70.0).is_ok());
    }

    #[test]
    fn rejects_inverted_thresholds_via_constructor() {
        // The only construction paths (`new`/`try_new`/`default`) all funnel
        // through this validation; the struct fields are private so an inverted
        // pair cannot be installed via a struct literal. This guards the dead
        // zone where the RSI could never cross either threshold.
        assert!(matches!(
            RsiThresholdParams::new(14, 70.0, 30.0),
            Err(RsiThresholdError::InvalidThresholdOrder { .. })
        ));
        assert!(matches!(
            RsiThresholdParams::new(14, 50.0, 50.0),
            Err(RsiThresholdError::InvalidThresholdOrder { .. })
        ));
    }

    #[test]
    fn out_of_order_detection_flags_backwards_bars_only() {
        let mut indicator = RsiThresholdIndicator::default();

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
        let mut indicator = RsiThresholdIndicator::default();
        let same_bar = bar("2026-01-01T00:00:00Z", 100.0);
        let _ = indicator.update(&same_bar, SignalPhase::Preview);
        assert!(!indicator.is_bar_out_of_order(&same_bar));
        let _ = indicator.update(&same_bar, SignalPhase::Confirmed);
        let later = bar("2026-01-01T00:01:00Z", 101.0);
        let _ = indicator.update(&later, SignalPhase::Confirmed);
    }
}
