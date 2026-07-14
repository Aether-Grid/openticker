use openticker_signals::{
    IndicatorBuildError, IndicatorDescriptor, IndicatorEngine, IndicatorManifest,
    builtin_indicator_descriptors,
};
use std::sync::OnceLock;
use toml::Table;

static INDICATOR_DESCRIPTORS: OnceLock<Vec<&'static IndicatorDescriptor>> = OnceLock::new();
static INDICATOR_MANIFESTS: OnceLock<Vec<IndicatorManifest>> = OnceLock::new();

/// Returns every registered indicator descriptor, sorted by type ID.
///
/// The aggregated registry is built once on first access and cached.
///
/// # Panics
///
/// Panics on first access if two indicator descriptors share the same
/// `type_id`. This is fail-fast startup validation: a duplicate type ID is a
/// build-time wiring error (two indicators claiming the same identifier), so
/// the registry refuses to serve an ambiguous mapping. The offending type ID is
/// logged via `tracing::error!` and included in the panic message so the
/// conflict is debuggable even though the panic can surface on any thread's
/// first access.
#[must_use]
pub fn indicator_descriptors() -> &'static [&'static IndicatorDescriptor] {
    INDICATOR_DESCRIPTORS
        .get_or_init(|| {
            let mut descriptors = builtin_indicator_descriptors().to_vec();
            #[cfg(feature = "indicators")]
            descriptors.extend(openticker_indicators::indicator_descriptors());

            descriptors.sort_by_key(|descriptor| descriptor.manifest.type_id);

            assert_no_duplicate_type_ids(
                descriptors
                    .iter()
                    .map(|descriptor| descriptor.manifest.type_id),
            );

            descriptors
        })
        .as_slice()
}

/// Panics if `type_ids` (which must be sorted) contains adjacent duplicates.
///
/// Logs the offending ID via `tracing::error!` before panicking so the conflict
/// is diagnosable from logs as well as the panic message.
///
/// # Panics
///
/// Panics on the first duplicated `type_id`.
fn assert_no_duplicate_type_ids<'a>(type_ids: impl IntoIterator<Item = &'a str>) {
    let mut last_type_id: Option<&str> = None;
    for type_id in type_ids {
        if last_type_id == Some(type_id) {
            // Log before panicking so the specific conflicting type ID is
            // captured even if the panic is later caught or the backtrace is
            // unavailable. The `tracing::error!` also keeps this from being a
            // bare `if { panic!() }` (clippy::manual_assert), which an `assert!`
            // could not express with this diagnostic.
            tracing::error!(
                duplicate_type_id = type_id,
                "duplicate indicator descriptor registered; refusing to build registry",
            );
            panic!("duplicate indicator descriptor registered for `{type_id}`");
        }
        last_type_id = Some(type_id);
    }
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

    #[test]
    fn assert_no_duplicate_type_ids_accepts_unique_ids() {
        // Must not panic on a sorted list of distinct IDs.
        assert_no_duplicate_type_ids(["alpha", "beta", "gamma"]);
    }

    #[test]
    #[should_panic(expected = "duplicate indicator descriptor registered for `beta`")]
    fn assert_no_duplicate_type_ids_panics_on_adjacent_duplicate() {
        // Mirrors the production fail-fast path: a duplicated (sorted-adjacent)
        // type ID panics with a message naming the offending ID.
        assert_no_duplicate_type_ids(["alpha", "beta", "beta", "gamma"]);
    }
}
