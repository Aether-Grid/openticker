use crate::registry::builtin_indicator_descriptors;
use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorRole, IndicatorStabilityClass, MarketType, SignalPhase,
};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorMarketSupport {
    pub equities: bool,
    pub crypto: bool,
}

impl IndicatorMarketSupport {
    pub const BOTH: Self = Self {
        equities: true,
        crypto: true,
    };

    #[must_use]
    pub const fn supports(self, market: MarketType) -> bool {
        match market {
            MarketType::Equities => self.equities,
            MarketType::Crypto => self.crypto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorCapabilities {
    pub supports_intrabar: bool,
    pub supports_preview: bool,
    pub supports_confirmed: bool,
}

impl IndicatorCapabilities {
    #[must_use]
    pub const fn supports_phase(self, phase: SignalPhase) -> bool {
        match phase {
            SignalPhase::Preview => self.supports_preview,
            SignalPhase::Confirmed => self.supports_confirmed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorWarmupRequirements {
    pub minimum_confirmed_bars: usize,
    pub recommended_backfill_bars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicatorManifest {
    pub type_id: &'static str,
    pub family: &'static str,
    pub role_default: IndicatorRole,
    pub allowed_roles: &'static [IndicatorRole],
    pub stability_class: IndicatorStabilityClass,
    pub market_support: IndicatorMarketSupport,
    pub capabilities: IndicatorCapabilities,
    pub warmup: IndicatorWarmupRequirements,
    pub metadata: IndicatorMetadataCapabilities,
}

impl IndicatorManifest {
    #[must_use]
    pub const fn supports_market(self, market: MarketType) -> bool {
        self.market_support.supports(market)
    }

    #[must_use]
    pub const fn supports_phase(self, phase: SignalPhase) -> bool {
        self.capabilities.supports_phase(phase)
    }
}

static INDICATOR_MANIFESTS: OnceLock<Vec<IndicatorManifest>> = OnceLock::new();

#[must_use]
pub fn indicator_manifests() -> &'static [IndicatorManifest] {
    INDICATOR_MANIFESTS
        .get_or_init(|| {
            builtin_indicator_descriptors()
                .iter()
                .map(|descriptor| *descriptor.manifest)
                .collect::<Vec<_>>()
        })
        .as_slice()
}

#[must_use]
pub fn indicator_manifest(type_id: &str) -> Option<&'static IndicatorManifest> {
    builtin_indicator_descriptors()
        .iter()
        .find(|descriptor| descriptor.manifest.type_id == type_id)
        .map(|descriptor| descriptor.manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn manifests_have_unique_non_empty_type_ids() {
        let mut type_ids = HashSet::new();
        for manifest in indicator_manifests() {
            assert!(!manifest.type_id.trim().is_empty());
            assert!(type_ids.insert(manifest.type_id));
        }
    }

    #[test]
    fn manifests_cover_all_exported_indicator_modules() {
        let expected = ["rsi_threshold", "sma_crossover"];

        for type_id in expected {
            assert!(indicator_manifest(type_id).is_some(), "missing {type_id}");
        }
        assert_eq!(indicator_manifests().len(), expected.len());
    }

    #[test]
    fn sma_crossover_manifest_is_primary_and_dual_phase() {
        let manifest = indicator_manifest("sma_crossover").unwrap();
        assert_eq!(manifest.role_default, IndicatorRole::PrimarySignal);
        assert_eq!(
            manifest.stability_class,
            IndicatorStabilityClass::StableOnClose
        );
        assert!(manifest.supports_market(MarketType::Equities));
        assert!(manifest.supports_market(MarketType::Crypto));
        assert!(manifest.supports_phase(SignalPhase::Preview));
        assert!(manifest.supports_phase(SignalPhase::Confirmed));
    }

    #[test]
    fn rsi_threshold_manifest_is_filter_and_dual_phase() {
        let manifest = indicator_manifest("rsi_threshold").unwrap();
        assert_eq!(manifest.role_default, IndicatorRole::Filter);
        assert_eq!(
            manifest.stability_class,
            IndicatorStabilityClass::StableOnClose
        );
        assert!(manifest.supports_phase(SignalPhase::Preview));
        assert!(manifest.supports_phase(SignalPhase::Confirmed));
    }

    #[test]
    fn manifests_include_default_role_in_allowed_roles() {
        for manifest in indicator_manifests() {
            assert!(manifest.allowed_roles.contains(&manifest.role_default));
        }
    }

    #[test]
    fn manifests_expose_valid_warmup_requirements() {
        for manifest in indicator_manifests() {
            assert!(manifest.warmup.minimum_confirmed_bars > 0);
            assert!(
                manifest.warmup.recommended_backfill_bars >= manifest.warmup.minimum_confirmed_bars
            );
        }
    }
}
