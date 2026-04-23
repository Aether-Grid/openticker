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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RsiThresholdParams {
    pub period: usize,
    pub oversold: f64,
    pub overbought: f64,
}

impl RsiThresholdParams {
    /// # Errors
    ///
    /// Returns [`RsiThresholdError`] when one or more parameter values are invalid.
    pub fn new(period: usize, oversold: f64, overbought: f64) -> Result<Self, RsiThresholdError> {
        if period == 0 {
            return Err(RsiThresholdError::InvalidPeriod(period));
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
    last_snapshot: Option<RsiThresholdSnapshot>,
}

impl RsiThresholdIndicator {
    #[must_use]
    pub fn new(params: RsiThresholdParams) -> Self {
        Self {
            rsi: Rsi::new(params.period),
            params,
            prev_rsi: None,
            last_snapshot: None,
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

    #[must_use]
    pub fn update(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> RsiThresholdSnapshot {
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

        let snapshot = RsiThresholdSnapshot {
            rsi,
            period: self.params.period,
            oversold: self.params.oversold,
            overbought: self.params.overbought,
            signal,
        };
        self.last_snapshot = Some(snapshot.clone());
        snapshot
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
}
