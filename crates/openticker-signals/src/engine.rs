use crate::manifest::IndicatorManifest;
use openticker_core::{IndicatorSignal, OhlcvBar, SignalMetadata, SignalPhase};
use thiserror::Error;
use toml::Table;

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

#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorEvaluation {
    pub signal: IndicatorSignal,
    pub metadata: SignalMetadata,
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum IndicatorBuildError {
    #[error("unsupported indicator type `{0}")]
    UnsupportedType(String),
    #[error("{0}")]
    InvalidParameters(String),
}

#[derive(Debug, Clone, Copy)]
pub struct IndicatorDescriptor {
    pub manifest: &'static IndicatorManifest,
    pub build: fn(&Table) -> Result<Box<dyn IndicatorEngine>, IndicatorBuildError>,
}
