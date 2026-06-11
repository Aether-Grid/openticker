use openticker_config::{IndicatorInstanceConfig, InstanceConfig, SignalMode};
use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorRole, IndicatorSignal, IndicatorSignalMetadataFilters,
    IndicatorSignalPolicy, OhlcvBar, SignalMetadata, SignalPhase,
};
use openticker_registry::{build_engine, indicator_manifest};
use openticker_signals::{IndicatorEngine, log_indicator_evaluation};
use openticker_strategy::{ConsensusLongOnlyStrategy, SingleIndicatorLongOnlyStrategy};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InstanceError {
    #[error("{0}")]
    InvalidConfiguration(String),
}

#[derive(Debug, Clone)]
pub struct EvaluatedIndicatorSignal {
    pub id: String,
    pub role: IndicatorRole,
    pub signal_policy: IndicatorSignalPolicy,
    pub signal: IndicatorSignal,
    pub metadata_capabilities: IndicatorMetadataCapabilities,
    pub metadata_filters: IndicatorSignalMetadataFilters,
    pub metadata: SignalMetadata,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct ConfiguredIndicatorRuntime {
    pub id: String,
    pub role: IndicatorRole,
    pub signal_policy: IndicatorSignalPolicy,
    pub metadata_capabilities: IndicatorMetadataCapabilities,
    pub metadata_filters: IndicatorSignalMetadataFilters,
    pub weight: f64,
    pub engine: Box<dyn IndicatorEngine>,
}

#[derive(Debug)]
pub enum RuntimeStrategyEngine {
    Single(SingleIndicatorLongOnlyStrategy),
    Consensus(ConsensusLongOnlyStrategy),
}

fn invalid_configuration(message: impl Into<String>) -> InstanceError {
    InstanceError::InvalidConfiguration(message.into())
}

fn evaluate_indicator_engine(
    engine: &mut dyn IndicatorEngine,
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> openticker_signals::IndicatorEvaluation {
    match phase {
        SignalPhase::Preview => {
            let mut preview_engine = engine.clone_engine();
            preview_engine.evaluate(bar, phase)
        }
        SignalPhase::Confirmed => engine.evaluate(bar, phase),
    }
}

/// Computes the warmup bar target for an instance based on enabled indicators.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when an enabled indicator
/// references an unknown indicator type.
pub fn required_warmup_bars(instance: &InstanceConfig) -> Result<usize, InstanceError> {
    let mut recommended_target = 0usize;
    for indicator in &instance.indicators {
        if !indicator.enabled {
            continue;
        }

        let manifest = indicator_manifest(&indicator.indicator_type).ok_or_else(|| {
            invalid_configuration(format!(
                "instance `{}` references unknown indicator type `{}`",
                instance.id, indicator.indicator_type
            ))
        })?;
        recommended_target = recommended_target.max(manifest.warmup.recommended_backfill_bars);
    }

    Ok(instance.warmup_target_bars.unwrap_or(recommended_target))
}

/// Builds enabled runtime indicator engines for an instance.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when no indicators are
/// enabled, an indicator type is unknown, or indicator parameters are invalid.
pub fn build_runtime_indicators(
    instance: &InstanceConfig,
) -> Result<Vec<ConfiguredIndicatorRuntime>, InstanceError> {
    let mut indicators = Vec::new();
    for indicator in &instance.indicators {
        if !indicator.enabled {
            continue;
        }

        let manifest = indicator_manifest(&indicator.indicator_type).ok_or_else(|| {
            invalid_configuration(format!(
                "instance `{}` references unknown indicator type `{}`",
                instance.id, indicator.indicator_type
            ))
        })?;

        let role = indicator.role.unwrap_or(manifest.role_default);
        let signal_policy = indicator
            .signal_policy
            .unwrap_or(default_signal_policy(instance.signal_mode));
        // Instance-level `ConfirmedOnly` is a hard override: it forces
        // `ConfirmedRequired` regardless of any per-indicator `PreviewAllowed`
        // policy. This is intentional so the instance-wide confirmed-only
        // contract cannot be weakened by an individual indicator's setting.
        let signal_policy = if matches!(instance.signal_mode, SignalMode::ConfirmedOnly) {
            IndicatorSignalPolicy::ConfirmedRequired
        } else {
            signal_policy
        };
        let weight = indicator.weight.unwrap_or(1.0);
        let engine = build_runtime_indicator_engine(&instance.id, indicator)?;

        indicators.push(ConfiguredIndicatorRuntime {
            id: indicator.id.clone(),
            role,
            signal_policy,
            metadata_capabilities: manifest.metadata,
            metadata_filters: indicator.metadata_filters.clone(),
            weight,
            engine,
        });
    }

    if indicators.is_empty() {
        return Err(invalid_configuration(format!(
            "instance `{}` must define at least one enabled indicator",
            instance.id
        )));
    }

    Ok(indicators)
}

/// Builds the runtime strategy engine configured for an instance.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the configured strategy
/// name is not supported.
pub fn build_runtime_strategy(
    instance: &InstanceConfig,
) -> Result<RuntimeStrategyEngine, InstanceError> {
    match instance.strategy.as_str() {
        "single_indicator_signal" => Ok(RuntimeStrategyEngine::Single(
            SingleIndicatorLongOnlyStrategy,
        )),
        "consensus" => Ok(RuntimeStrategyEngine::Consensus(
            ConsensusLongOnlyStrategy::default(),
        )),
        other => Err(invalid_configuration(format!(
            "instance `{}` uses unsupported strategy `{other}`",
            instance.id
        ))),
    }
}

fn build_runtime_indicator_engine(
    instance_id: &str,
    indicator: &IndicatorInstanceConfig,
) -> Result<Box<dyn IndicatorEngine>, InstanceError> {
    build_engine(&indicator.indicator_type, &indicator.params).map_err(|error| {
        invalid_configuration(format!(
            "instance `{instance_id}` indicator `{}`: {error}",
            indicator.id
        ))
    })
}

#[must_use]
pub fn evaluate_indicator_signals(
    indicators: &mut [ConfiguredIndicatorRuntime],
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> Vec<EvaluatedIndicatorSignal> {
    indicators
        .iter_mut()
        .map(|indicator| {
            let evaluation = evaluate_indicator_engine(indicator.engine.as_mut(), bar, phase);
            log_indicator_evaluation(
                indicator.id.as_str(),
                indicator.engine.type_id(),
                phase,
                evaluation.signal,
                bar.close,
            );

            EvaluatedIndicatorSignal {
                id: indicator.id.clone(),
                role: indicator.role,
                signal_policy: indicator.signal_policy,
                signal: evaluation.signal,
                metadata_capabilities: indicator.metadata_capabilities,
                metadata_filters: indicator.metadata_filters.clone(),
                metadata: evaluation.metadata,
                weight: indicator.weight,
            }
        })
        .collect::<Vec<_>>()
}

#[must_use]
pub fn representative_indicator(
    indicators: &[EvaluatedIndicatorSignal],
) -> Option<&EvaluatedIndicatorSignal> {
    indicators
        .iter()
        .find(|indicator| indicator.role == IndicatorRole::PrimarySignal)
        .or_else(|| indicators.first())
}

#[must_use]
pub fn default_signal_policy(signal_mode: SignalMode) -> IndicatorSignalPolicy {
    match signal_mode {
        SignalMode::Intrabar => IndicatorSignalPolicy::PreviewAllowed,
        SignalMode::ConfirmedOnly => IndicatorSignalPolicy::ConfirmedRequired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use openticker_config::{
        BudgetConfig, ExecutionConstraintsConfig, InstanceRiskConfig, RiskOverrides,
    };
    use openticker_core::{MarketType, Timeframe};
    use openticker_registry::indicator_manifests;
    use toml::Table;

    fn test_instance(indicator_type: &str) -> InstanceConfig {
        InstanceConfig {
            id: "test-instance".to_owned(),
            enabled: true,
            market: MarketType::Equities,
            symbols: vec!["AAPL".to_owned()],
            timeframe: Timeframe::M1,
            account: "paper-equities".to_owned(),
            data_connector: "alpaca-paper".to_owned(),
            execution_connector: "alpaca-paper".to_owned(),
            strategy: "single_indicator_signal".to_owned(),
            signal_mode: SignalMode::Intrabar,
            polling_enabled: true,
            polling_interval_ms: 1_000,
            indicators: vec![IndicatorInstanceConfig {
                id: "test-indicator".to_owned(),
                indicator_type: indicator_type.to_owned(),
                enabled: true,
                role: None,
                signal_policy: None,
                weight: None,
                metadata_filters: IndicatorSignalMetadataFilters::default(),
                params: Table::new(),
            }],
            execution_constraints: ExecutionConstraintsConfig::default(),
            budget: BudgetConfig { pct: 100.0 },
            risk: InstanceRiskConfig {
                profile: "equities-default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: None,
            allow_live: false,
        }
    }

    fn test_bar_at(timestamp: &str, close: f64) -> OhlcvBar {
        OhlcvBar {
            timestamp: DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc),
            open: close,
            high: close + 0.9,
            low: close - 0.9,
            close,
            volume: 1_000.0,
        }
    }

    #[test]
    fn preview_updates_do_not_mutate_committed_indicator_history() {
        let bar_1 = test_bar_at("2030-01-01T00:00:00Z", 100.0);
        let bar_2_preview_a = test_bar_at("2030-01-01T00:01:00Z", 101.0);
        let bar_2_preview_b = test_bar_at("2030-01-01T00:01:00Z", 104.0);
        let bar_2_confirmed = test_bar_at("2030-01-01T00:01:00Z", 102.5);
        let bar_3 = test_bar_at("2030-01-01T00:02:00Z", 103.0);

        for indicator_type in ["sma_crossover", "rsi_threshold"] {
            let instance = test_instance(indicator_type);
            let mut path_with_previews = build_runtime_indicators(&instance).unwrap();
            let mut confirmed_only = build_runtime_indicators(&instance).unwrap();

            let _ =
                evaluate_indicator_signals(&mut path_with_previews, &bar_1, SignalPhase::Confirmed);
            let _ = evaluate_indicator_signals(
                &mut path_with_previews,
                &bar_2_preview_a,
                SignalPhase::Preview,
            );
            let _ = evaluate_indicator_signals(
                &mut path_with_previews,
                &bar_2_preview_b,
                SignalPhase::Preview,
            );
            let _ = evaluate_indicator_signals(
                &mut path_with_previews,
                &bar_2_confirmed,
                SignalPhase::Confirmed,
            );

            let _ = evaluate_indicator_signals(&mut confirmed_only, &bar_1, SignalPhase::Confirmed);
            let _ = evaluate_indicator_signals(
                &mut confirmed_only,
                &bar_2_confirmed,
                SignalPhase::Confirmed,
            );

            assert_eq!(
                format!("{:?}", path_with_previews[0].engine),
                format!("{:?}", confirmed_only[0].engine),
                "indicator `{indicator_type}` committed state diverged after preview updates"
            );

            let next_with_previews =
                evaluate_indicator_signals(&mut path_with_previews, &bar_3, SignalPhase::Confirmed);
            let next_confirmed_only =
                evaluate_indicator_signals(&mut confirmed_only, &bar_3, SignalPhase::Confirmed);
            assert_eq!(
                next_with_previews[0].signal, next_confirmed_only[0].signal,
                "indicator `{indicator_type}` next confirmed signal diverged after preview updates"
            );
        }
    }

    #[test]
    fn all_manifest_supported_indicator_types_construct_or_fail_explicitly() {
        for manifest in indicator_manifests() {
            let instance = test_instance(manifest.type_id);
            match build_runtime_indicators(&instance) {
                Ok(indicators) => {
                    assert_eq!(
                        indicators.len(),
                        1,
                        "manifest `{}` should build exactly one runtime indicator",
                        manifest.type_id
                    );
                    assert_eq!(indicators[0].engine.type_id(), manifest.type_id);
                }
                Err(error) => {
                    let message = error.to_string();
                    assert!(
                        message.contains("instance `test-instance`"),
                        "manifest `{}` should fail with contextual instance id: {message}",
                        manifest.type_id
                    );
                    assert!(
                        message.contains("indicator `test-indicator`"),
                        "manifest `{}` should fail with contextual indicator id: {message}",
                        manifest.type_id
                    );
                    assert!(
                        message.contains(manifest.type_id),
                        "manifest `{}` should fail with explicit indicator type: {message}",
                        manifest.type_id
                    );
                }
            }
        }
    }

    #[test]
    fn confirmed_only_policy_suppresses_preview_outputs() {
        let mut instance = test_instance("sma_crossover");
        instance.signal_mode = SignalMode::ConfirmedOnly;
        instance.indicators[0].signal_policy = Some(IndicatorSignalPolicy::PreviewAllowed);

        let indicators = build_runtime_indicators(&instance).unwrap();
        assert_eq!(
            indicators[0].signal_policy,
            IndicatorSignalPolicy::ConfirmedRequired
        );
    }

    #[test]
    fn required_warmup_bars_uses_highest_manifest_requirement() {
        let mut instance = test_instance("sma_crossover");
        instance.indicators.push(IndicatorInstanceConfig {
            id: "warmup-second".to_owned(),
            indicator_type: "rsi_threshold".to_owned(),
            enabled: true,
            role: None,
            signal_policy: None,
            weight: None,
            metadata_filters: IndicatorSignalMetadataFilters::default(),
            params: Table::new(),
        });

        let primary = indicator_manifest("sma_crossover").unwrap();
        let secondary = indicator_manifest("rsi_threshold").unwrap();
        let expected_max = primary
            .warmup
            .recommended_backfill_bars
            .max(secondary.warmup.recommended_backfill_bars);

        assert_eq!(required_warmup_bars(&instance).unwrap(), expected_max);

        instance.warmup_target_bars = Some(expected_max + 25);
        assert_eq!(required_warmup_bars(&instance).unwrap(), expected_max + 25);
    }
}
