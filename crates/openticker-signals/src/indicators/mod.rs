use crate::IndicatorDescriptor;

pub mod rsi_threshold;
pub mod sma_crossover;

pub const BUILTIN_INDICATOR_DESCRIPTORS: &[&IndicatorDescriptor] =
    &[&sma_crossover::DESCRIPTOR, &rsi_threshold::DESCRIPTOR];
