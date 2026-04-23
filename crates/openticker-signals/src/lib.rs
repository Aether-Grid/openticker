use openticker_core::{IndicatorSignal, OhlcvBar, SignalMetadata, SignalPhase};
use thiserror::Error;
use toml::Table;

mod common;
mod observability;

pub mod indicators;
pub mod manifest;
pub mod registry;

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

#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorEvaluation {
    pub signal: IndicatorSignal,
    pub metadata: SignalMetadata,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IndicatorBuildError {
    #[error("unsupported indicator type `{0}`")]
    UnsupportedType(String),
    #[error("{0}")]
    InvalidParameters(String),
}

#[derive(Debug, Clone, Copy)]
pub struct IndicatorDescriptor {
    pub manifest: &'static IndicatorManifest,
    pub build: fn(&Table) -> Result<Box<dyn IndicatorEngine>, IndicatorBuildError>,
}

impl IndicatorEvaluation {
    #[must_use]
    pub fn from_snapshot(snapshot: &impl SignalSnapshot) -> Self {
        Self {
            signal: snapshot.signal(),
            metadata: snapshot.metadata(),
        }
    }
}

pub trait IndicatorEngine: Send + Sync + std::fmt::Debug {
    fn type_id(&self) -> &'static str;

    fn evaluate(&mut self, bar: &OhlcvBar, phase: SignalPhase) -> IndicatorEvaluation;

    fn clone_engine(&self) -> Box<dyn IndicatorEngine>;
}

impl Clone for Box<dyn IndicatorEngine> {
    fn clone(&self) -> Self {
        self.clone_engine()
    }
}

pub trait SignalSnapshot {
    fn signal(&self) -> IndicatorSignal;

    fn metadata(&self) -> SignalMetadata;
}
