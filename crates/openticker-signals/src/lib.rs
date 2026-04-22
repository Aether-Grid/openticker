use openticker_core::{IndicatorSignal, OhlcvBar, SignalMetadata, SignalPhase};

mod common;
mod observability;

pub mod manifest;
pub mod signals;

pub use signals::{rsi_threshold, sma_crossover};

pub use manifest::{
    IndicatorCapabilities, IndicatorManifest, IndicatorMarketSupport, IndicatorWarmupRequirements,
    indicator_manifest, indicator_manifests,
};
pub use observability::log_indicator_evaluation;
pub use openticker_core::{
    IndicatorFactValue, IndicatorMetadataCapabilities, IndicatorRole,
    IndicatorSignalMetadataFilters, IndicatorSignalPolicy, IndicatorStabilityClass,
    IndicatorTradeLevels, SignalStrength,
};

pub trait IndicatorEngine {
    type Snapshot;

    fn update(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> Self::Snapshot;
}

pub trait SignalSnapshot {
    fn signal(&self) -> IndicatorSignal;

    fn metadata(&self) -> SignalMetadata;
}
