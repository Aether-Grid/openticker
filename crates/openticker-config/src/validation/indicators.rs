//! Indicator validation rules: manifest lookup, roles, signal policies,
//! stability classes, weights, and warmup requirements.

use super::connectors::market_type_label;
use crate::error::ConfigError;
use crate::model::{InstanceConfig, SignalMode};
use openticker_core::{IndicatorRole, IndicatorSignalPolicy, IndicatorStabilityClass};
use openticker_registry::indicator_manifest;
use std::collections::HashSet;

#[allow(clippy::too_many_lines)]
pub(super) fn validate_indicators(
    instance: &InstanceConfig,
    live_account: bool,
) -> Result<(), ConfigError> {
    // NOTE: `indicator.params` is intentionally NOT validated structurally here.
    // The `toml::Table` type already guarantees the section is a table (a
    // non-table value fails at deserialization), but per-key shape/type
    // validation requires the per-indicator parameter schemas, which live in the
    // indicator crate (behind the `indicators` feature) and are unavailable to
    // this crate without a dependency cycle. Params are validated when an engine
    // is built via `openticker_registry::build_engine`. See the doc comment on
    // `IndicatorInstanceConfig::params` for the full contract.
    let mut indicator_ids = HashSet::new();
    let mut minimum_warmup_bars = 0usize;
    for indicator in &instance.indicators {
        if indicator.id.trim().is_empty() {
            return Err(ConfigError::validation(format!(
                "instance `{}` has an indicator with an empty id",
                instance.id
            )));
        }
        if !indicator_ids.insert(indicator.id.clone()) {
            return Err(ConfigError::validation(format!(
                "instance `{}` has duplicate indicator id `{}`",
                instance.id, indicator.id
            )));
        }
        let Some(manifest) = indicator_manifest(indicator.indicator_type.as_str()) else {
            return Err(ConfigError::validation(format!(
                "instance `{}` has unsupported indicator type `{}`",
                instance.id, indicator.indicator_type
            )));
        };

        minimum_warmup_bars = minimum_warmup_bars.max(manifest.warmup.minimum_confirmed_bars);

        let effective_role = indicator.role.unwrap_or(manifest.role_default);
        if !manifest.allowed_roles.contains(&effective_role) {
            return Err(ConfigError::validation(format!(
                "instance `{}` indicator `{}` type `{}` does not allow configured role `{}`",
                instance.id,
                indicator.id,
                indicator.indicator_type,
                indicator_role_label(effective_role),
            )));
        }

        if !manifest.supports_market(instance.market) {
            return Err(ConfigError::validation(format!(
                "instance `{}` indicator `{}` type `{}` does not support market `{}`",
                instance.id,
                indicator.id,
                indicator.indicator_type,
                market_type_label(instance.market)
            )));
        }

        match instance.signal_mode {
            SignalMode::Intrabar => {
                if !manifest.capabilities.supports_intrabar
                    || !manifest.capabilities.supports_preview
                {
                    return Err(ConfigError::validation(format!(
                        "instance `{}` indicator `{}` type `{}` does not support intrabar preview mode",
                        instance.id, indicator.id, indicator.indicator_type
                    )));
                }
            }
            SignalMode::ConfirmedOnly => {
                if !manifest.capabilities.supports_confirmed {
                    return Err(ConfigError::validation(format!(
                        "instance `{}` indicator `{}` type `{}` does not support confirmed-only mode",
                        instance.id, indicator.id, indicator.indicator_type
                    )));
                }
            }
        }

        if effective_role == IndicatorRole::PrimarySignal && live_account {
            match manifest.stability_class {
                IndicatorStabilityClass::ParityOnlyUnsafe
                | IndicatorStabilityClass::ZigzagRevisable => {
                    return Err(ConfigError::validation(format!(
                        "instance `{}` indicator `{}` type `{}` with stability_class `{}` cannot run as `primary_signal` in live mode",
                        instance.id,
                        indicator.id,
                        indicator.indicator_type,
                        indicator_stability_class_label(manifest.stability_class),
                    )));
                }
                IndicatorStabilityClass::StableOnClose
                | IndicatorStabilityClass::PreviewOnly
                | IndicatorStabilityClass::PivotDelayed => {}
            }
        }

        if let Some(signal_policy) = indicator.signal_policy {
            if matches!(instance.signal_mode, SignalMode::ConfirmedOnly)
                && matches!(signal_policy, IndicatorSignalPolicy::PreviewAllowed)
            {
                return Err(ConfigError::validation(format!(
                    "instance `{}` indicator `{}` cannot use signal_policy `preview_allowed` when signal_mode is `confirmed_only`",
                    instance.id, indicator.id
                )));
            }

            match signal_policy {
                IndicatorSignalPolicy::PreviewAllowed => {
                    if !manifest.capabilities.supports_intrabar
                        || !manifest.capabilities.supports_preview
                    {
                        return Err(ConfigError::validation(format!(
                            "instance `{}` indicator `{}` type `{}` cannot use signal_policy `preview_allowed`",
                            instance.id, indicator.id, indicator.indicator_type
                        )));
                    }
                }
                IndicatorSignalPolicy::ConfirmedRequired => {
                    if !manifest.capabilities.supports_confirmed {
                        return Err(ConfigError::validation(format!(
                            "instance `{}` indicator `{}` type `{}` cannot use signal_policy `confirmed_required`",
                            instance.id, indicator.id, indicator.indicator_type
                        )));
                    }
                }
            }
        }

        if let Some(weight) = indicator.weight {
            if !weight.is_finite() || weight <= 0.0 {
                return Err(ConfigError::validation(format!(
                    "instance `{}` indicator `{}` has invalid weight `{weight}`",
                    instance.id, indicator.id
                )));
            }
            if instance.strategy != "consensus" {
                return Err(ConfigError::validation(format!(
                    "instance `{}` indicator `{}` sets weight but strategy `{}` is not `consensus`",
                    instance.id, indicator.id, instance.strategy
                )));
            }
        }
    }

    if let Some(target_bars) = instance.warmup_target_bars
        && target_bars < minimum_warmup_bars
    {
        return Err(ConfigError::validation(format!(
            "instance `{}` warmup_target_bars `{target_bars}` is below the required minimum `{minimum_warmup_bars}` for its enabled indicators",
            instance.id
        )));
    }

    Ok(())
}

fn indicator_role_label(role: IndicatorRole) -> &'static str {
    match role {
        IndicatorRole::PrimarySignal => "primary_signal",
        IndicatorRole::Filter => "filter",
        IndicatorRole::Context => "context",
        IndicatorRole::RiskHelper => "risk_helper",
        IndicatorRole::ResearchOnly => "research_only",
    }
}

fn indicator_stability_class_label(class: IndicatorStabilityClass) -> &'static str {
    match class {
        IndicatorStabilityClass::StableOnClose => "stable_on_close",
        IndicatorStabilityClass::PreviewOnly => "preview_only",
        IndicatorStabilityClass::PivotDelayed => "pivot_delayed",
        IndicatorStabilityClass::ZigzagRevisable => "zigzag_revisable",
        IndicatorStabilityClass::ParityOnlyUnsafe => "parity_only_unsafe",
    }
}
