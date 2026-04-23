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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmaCrossoverParams {
    pub fast_length: usize,
    pub slow_length: usize,
}

impl SmaCrossoverParams {
    /// # Errors
    ///
    /// Returns [`SmaCrossoverError`] when one or more parameter values are invalid.
    pub fn new(fast_length: usize, slow_length: usize) -> Result<Self, SmaCrossoverError> {
        if fast_length == 0 {
            return Err(SmaCrossoverError::InvalidFastLength(fast_length));
        }
        if slow_length == 0 {
            return Err(SmaCrossoverError::InvalidSlowLength(slow_length));
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
    last_snapshot: Option<SmaCrossoverSnapshot>,
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
            last_snapshot: None,
        }
    }

    /// # Errors
    ///
    /// Returns [`SmaCrossoverError`] when one or more parameter values are invalid.
    pub fn try_new(fast_length: usize, slow_length: usize) -> Result<Self, SmaCrossoverError> {
        SmaCrossoverParams::new(fast_length, slow_length).map(Self::new)
    }

    #[must_use]
    pub fn update(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> SmaCrossoverSnapshot {
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

        let snapshot = SmaCrossoverSnapshot {
            fast_sma,
            slow_sma,
            fast_length: self.params.fast_length,
            slow_length: self.params.slow_length,
            signal,
        };
        self.last_snapshot = Some(snapshot.clone());
        snapshot
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
}
