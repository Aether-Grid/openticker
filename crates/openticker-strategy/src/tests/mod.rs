mod consensus;
mod single_indicator;

use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorSignalMetadataFilters, SignalMetadata,
};

fn metadata() -> SignalMetadata {
    SignalMetadata::default()
}

fn metadata_filters() -> IndicatorSignalMetadataFilters {
    IndicatorSignalMetadataFilters::default()
}

fn metadata_capabilities() -> IndicatorMetadataCapabilities {
    IndicatorMetadataCapabilities::default()
}
