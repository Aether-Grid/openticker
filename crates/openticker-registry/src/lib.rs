use openticker_signals::{
    IndicatorBuildError, IndicatorDescriptor, IndicatorEngine, IndicatorManifest,
    builtin_indicator_descriptors,
};
use std::sync::OnceLock;
use toml::Table;

static INDICATOR_DESCRIPTORS: OnceLock<Vec<&'static IndicatorDescriptor>> = OnceLock::new();
static INDICATOR_MANIFESTS: OnceLock<Vec<IndicatorManifest>> = OnceLock::new();

#[must_use]
pub fn indicator_descriptors() -> &'static [&'static IndicatorDescriptor] {
    INDICATOR_DESCRIPTORS
        .get_or_init(|| {
            let mut descriptors = builtin_indicator_descriptors().to_vec();
            #[cfg(feature = "indicators")]
            descriptors.extend(openticker_indicators::indicator_descriptors());

            descriptors.sort_by_key(|descriptor| descriptor.manifest.type_id);

            let mut last_type_id = None;
            for descriptor in &descriptors {
                if last_type_id == Some(descriptor.manifest.type_id) {
                    panic!(
                        "duplicate indicator descriptor registered for `{}`",
                        descriptor.manifest.type_id
                    );
                }
                last_type_id = Some(descriptor.manifest.type_id);
            }

            descriptors
        })
        .as_slice()
}

#[must_use]
pub fn indicator_descriptor(type_id: &str) -> Option<&'static IndicatorDescriptor> {
    indicator_descriptors()
        .iter()
        .find(|descriptor| descriptor.manifest.type_id == type_id)
        .copied()
}

#[must_use]
pub fn indicator_manifests() -> &'static [IndicatorManifest] {
    INDICATOR_MANIFESTS
        .get_or_init(|| {
            indicator_descriptors()
                .iter()
                .map(|descriptor| *descriptor.manifest)
                .collect::<Vec<_>>()
        })
        .as_slice()
}

#[must_use]
pub fn indicator_manifest(type_id: &str) -> Option<&'static IndicatorManifest> {
    indicator_descriptor(type_id).map(|descriptor| descriptor.manifest)
}

/// # Errors
///
/// Returns [`IndicatorBuildError`] when the indicator type is unknown or its
/// params are invalid.
pub fn build_engine(
    type_id: &str,
    params: &Table,
) -> Result<Box<dyn IndicatorEngine>, IndicatorBuildError> {
    let descriptor = indicator_descriptor(type_id)
        .ok_or_else(|| IndicatorBuildError::UnsupportedType(type_id.to_owned()))?;
    (descriptor.build)(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregated_registry_has_unique_type_ids() {
        let mut type_ids = indicator_descriptors()
            .iter()
            .map(|descriptor| descriptor.manifest.type_id)
            .collect::<Vec<_>>();
        let original_len = type_ids.len();
        type_ids.sort_unstable();
        type_ids.dedup();
        assert_eq!(type_ids.len(), original_len);
    }
}
