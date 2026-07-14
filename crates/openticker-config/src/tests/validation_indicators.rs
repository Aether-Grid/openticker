//! Validation tests for indicator rules.

use super::support::{
    connector_validation_bundle, default_validation_account, default_validation_instance,
    intrabar_validation_account, intrabar_validation_instance,
};
use crate::ConfigError;
use openticker_core::{IndicatorRole, IndicatorSignalPolicy};

#[test]
fn accepts_registered_indicator_type_from_manifest() {
    let mut instance = intrabar_validation_instance();
    instance.indicators[0].indicator_type = "rsi_threshold".to_owned();

    let bundle = connector_validation_bundle(intrabar_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_unknown_indicator_type_from_manifest_registry() {
    let mut instance = default_validation_instance();
    instance.indicators[0].indicator_type = "unknown_indicator".to_owned();

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("unsupported indicator type")
    ));
}

#[test]
fn rejects_disallowed_role_override() {
    let mut instance = default_validation_instance();
    instance.indicators[0].role = Some(IndicatorRole::Filter);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("does not allow configured role")
    ));
}

#[test]
fn rejects_preview_policy_when_signal_mode_is_confirmed_only() {
    let mut instance = default_validation_instance();
    instance.indicators[0].indicator_type = "sma_crossover".to_owned();
    instance.indicators[0].signal_policy = Some(IndicatorSignalPolicy::PreviewAllowed);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("signal_mode is `confirmed_only`")
    ));
}

#[test]
fn rejects_weight_when_strategy_is_not_consensus() {
    let mut instance = default_validation_instance();
    instance.indicators[0].weight = Some(1.0);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message })
            if message.contains("sets weight but strategy")
    ));
}

#[test]
fn accepts_weight_for_consensus_strategy() {
    let mut instance = default_validation_instance();
    instance.strategy = "consensus".to_owned();
    instance.indicators[0].weight = Some(1.5);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn accepts_indicator_metadata_filters_without_capability_requirements() {
    let mut instance = default_validation_instance();
    instance.indicators[0]
        .metadata_filters
        .entry
        .allowed_strengths = vec![openticker_core::SignalStrength::Strong];
    instance.indicators[0]
        .metadata_filters
        .entry
        .allowed_reason_codes = vec!["supertrend_cross_up_sma_filter".to_owned()];

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_warmup_target_bars_below_indicator_minimum() {
    let mut instance = default_validation_instance();
    instance.warmup_target_bars = Some(10);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("warmup_target_bars")
    ));
}

#[test]
fn accepts_warmup_target_bars_at_or_above_indicator_minimum() {
    let mut instance = default_validation_instance();
    instance.warmup_target_bars = Some(50);

    let bundle = connector_validation_bundle(default_validation_account(), instance);
    assert!(bundle.validate().is_ok());
}
