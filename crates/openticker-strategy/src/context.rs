use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorRole, IndicatorSignal, IndicatorSignalMetadataFilters,
    IndicatorSignalPolicy, SignalMetadata,
};

#[derive(Debug, Clone)]
pub struct StrategyContext<'a> {
    pub indicator_id: &'a str,
    pub signal: IndicatorSignal,
    pub signal_policy: IndicatorSignalPolicy,
    pub metadata_capabilities: IndicatorMetadataCapabilities,
    pub metadata_filters: &'a IndicatorSignalMetadataFilters,
    pub metadata: &'a SignalMetadata,
    pub has_position: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct IndicatorObservation<'a> {
    pub id: &'a str,
    pub role: IndicatorRole,
    pub signal_policy: IndicatorSignalPolicy,
    pub signal: IndicatorSignal,
    pub metadata_capabilities: IndicatorMetadataCapabilities,
    pub metadata_filters: &'a IndicatorSignalMetadataFilters,
    pub metadata: &'a SignalMetadata,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ConsensusStrategyContext<'a> {
    pub indicators: &'a [IndicatorObservation<'a>],
    pub has_position: bool,
}
