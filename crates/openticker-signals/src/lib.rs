use openticker_core::IndicatorSignal;

mod common;
mod engine;
mod observability;

pub mod indicators;
pub mod manifest;
pub mod registry;

pub use engine::{
    IndicatorBuildError, IndicatorDescriptor, IndicatorEngine, IndicatorEvaluation, SignalSnapshot,
};
pub use indicators::{rsi_threshold, sma_crossover};
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
pub use registry::{
    build_builtin_engine, builtin_indicator_descriptor, builtin_indicator_descriptors,
};
