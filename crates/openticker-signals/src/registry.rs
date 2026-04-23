use crate::{IndicatorBuildError, IndicatorDescriptor, IndicatorEngine, indicators};
use toml::Table;

#[must_use]
pub fn builtin_indicator_descriptors() -> &'static [&'static IndicatorDescriptor] {
    indicators::BUILTIN_INDICATOR_DESCRIPTORS
}

#[must_use]
pub fn builtin_indicator_descriptor(type_id: &str) -> Option<&'static IndicatorDescriptor> {
    builtin_indicator_descriptors()
        .iter()
        .find(|descriptor| descriptor.manifest.type_id == type_id)
        .copied()
}

/// # Errors
///
/// Returns [`IndicatorBuildError`] when the indicator type is unknown or its
/// params are invalid.
pub fn build_builtin_engine(
    type_id: &str,
    params: &Table,
) -> Result<Box<dyn IndicatorEngine>, IndicatorBuildError> {
    let descriptor = builtin_indicator_descriptor(type_id)
        .ok_or_else(|| IndicatorBuildError::UnsupportedType(type_id.to_owned()))?;
    (descriptor.build)(params)
}
