use crate::position::inventory_state_from_runtime_fields;
use crate::state::{LaneRecoveryState, LaneRuntime, LaneRuntimeState};
use crate::warmup::InstanceWarmupState;
use openticker_config::InstanceConfig;
use openticker_core::{BotLaneKey, ExecutionMode};
use openticker_data::BarBuilder;
use openticker_instance::{ConfiguredIndicatorRuntime, InstanceError, RuntimeStrategyEngine};
use openticker_risk::RiskLimits;
use std::{collections::HashMap, hash::BuildHasher};

pub type RuntimeLaneBuild = (HashMap<String, LaneRuntime>, HashMap<String, Vec<String>>);

/// Computes the warmup bar target for a lane based on enabled indicators.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when an enabled indicator
/// references an unknown indicator type.
pub fn required_warmup_bars(instance: &InstanceConfig) -> Result<usize, InstanceError> {
    openticker_instance::required_warmup_bars(instance)
}

/// Builds enabled runtime indicator engines for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when no indicators are
/// enabled, an indicator type is unknown, or indicator parameters are invalid.
pub fn build_runtime_indicators(
    instance: &InstanceConfig,
) -> Result<Vec<ConfiguredIndicatorRuntime>, InstanceError> {
    openticker_instance::build_runtime_indicators(instance)
}

/// Builds the runtime strategy engine for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the configured strategy
/// is unsupported.
pub fn build_runtime_strategy(
    instance: &InstanceConfig,
) -> Result<RuntimeStrategyEngine, InstanceError> {
    openticker_instance::build_runtime_strategy(instance)
}

#[derive(Debug, Clone, Copy)]
pub struct RecoveredInstanceState {
    pub state: LaneRuntimeState,
    pub resume_after_startup_reconcile: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RiskProfileRuntimeConfig {
    pub limits: RiskLimits,
    pub target_order_notional_usd: f64,
}

#[must_use]
pub fn recover_lane_state(
    recovered: LaneRuntimeState,
    default_start_paused_if_recovery_uncertain: bool,
) -> LaneRuntimeState {
    if default_start_paused_if_recovery_uncertain && matches!(recovered, LaneRuntimeState::Running)
    {
        LaneRuntimeState::Reconciling
    } else {
        recovered
    }
}

/// Resolves the lane identifier for an instance/symbol pair.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the lane identity cannot
/// be encoded for a multi-symbol bot.
pub fn lane_instance_id(instance: &InstanceConfig, symbol: &str) -> Result<String, InstanceError> {
    if instance.symbols.len() == 1 {
        Ok(instance.id.clone())
    } else {
        BotLaneKey::parse(instance.id.clone(), symbol.to_owned())
            .map(|lane_key| lane_key.encoded())
            .map_err(|error| {
                InstanceError::InvalidConfiguration(format!(
                    "invalid lane identity for instance `{}` symbol `{symbol}`: {error}",
                    instance.id
                ))
            })
    }
}

#[must_use]
pub fn resolved_instance_state(
    instance: &InstanceConfig,
    snapshot_states: &HashMap<String, String, impl BuildHasher>,
    default_start_paused_if_recovery_uncertain: bool,
) -> RecoveredInstanceState {
    let default_state = if instance.enabled {
        LaneRuntimeState::Stopped
    } else {
        LaneRuntimeState::Paused
    };

    let persisted_state = snapshot_states
        .get(&instance.id)
        .and_then(|state| LaneRuntimeState::from_storage_value(state));

    let recovered_state = match persisted_state {
        Some(parsed) => recover_lane_state(parsed, default_start_paused_if_recovery_uncertain),
        None if snapshot_states.contains_key(&instance.id)
            && default_start_paused_if_recovery_uncertain =>
        {
            LaneRuntimeState::Reconciling
        }
        None => default_state,
    };

    let state = if instance.enabled {
        recovered_state
    } else {
        LaneRuntimeState::Paused
    };

    RecoveredInstanceState {
        state,
        resume_after_startup_reconcile: instance.enabled
            && default_start_paused_if_recovery_uncertain
            && matches!(persisted_state, Some(LaneRuntimeState::Running))
            && matches!(state, LaneRuntimeState::Reconciling),
    }
}

/// Resolves effective risk limits for a lane by applying instance overrides on
/// top of the referenced risk profile.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the referenced risk
/// profile is missing.
pub fn resolved_risk_limits(
    instance: &InstanceConfig,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
) -> Result<RiskLimits, InstanceError> {
    let base_limits = risk_profiles_by_id
        .get(&instance.risk.profile)
        .map(|profile| profile.limits)
        .ok_or_else(|| {
            InstanceError::InvalidConfiguration(format!(
                "instance `{}` references unknown risk profile `{}`",
                instance.id, instance.risk.profile
            ))
        })?;

    Ok(RiskLimits {
        max_daily_loss_pct: instance
            .risk
            .overrides
            .max_daily_loss_pct
            .unwrap_or(base_limits.max_daily_loss_pct),
        max_open_positions: instance
            .risk
            .overrides
            .max_open_positions
            .unwrap_or(base_limits.max_open_positions),
        max_order_notional_usd: instance
            .risk
            .overrides
            .max_order_notional_usd
            .unwrap_or(base_limits.max_order_notional_usd),
        max_spread_bps: instance
            .risk
            .overrides
            .max_spread_bps
            .unwrap_or(base_limits.max_spread_bps),
        max_slippage_bps: instance
            .risk
            .overrides
            .max_slippage_bps
            .unwrap_or(base_limits.max_slippage_bps),
        stale_data_ms: instance
            .risk
            .overrides
            .stale_data_ms
            .unwrap_or(base_limits.stale_data_ms),
        cooldown_after_reject_ms: instance
            .risk
            .overrides
            .cooldown_after_reject_ms
            .unwrap_or(base_limits.cooldown_after_reject_ms),
    })
}

/// Resolves the effective target order notional for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the referenced risk
/// profile is missing.
pub fn resolved_target_order_notional_usd(
    instance: &InstanceConfig,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
) -> Result<f64, InstanceError> {
    let base_target = risk_profiles_by_id
        .get(&instance.risk.profile)
        .map(|profile| profile.target_order_notional_usd)
        .ok_or_else(|| {
            InstanceError::InvalidConfiguration(format!(
                "instance `{}` references unknown risk profile `{}`",
                instance.id, instance.risk.profile
            ))
        })?;

    Ok(instance
        .risk
        .overrides
        .target_order_notional_usd
        .unwrap_or(base_target))
}

/// Builds the mutable lane runtime state from config, recovery state, and
/// recovered ledger signals.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the lane wiring cannot be
/// built from config.
pub fn build_lane_runtime(
    instance: &InstanceConfig,
    symbol: &str,
    state: LaneRuntimeState,
    resume_after_startup_reconcile: bool,
    execution_mode: ExecutionMode,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
    recovered_realized_pnl_usd: f64,
) -> Result<LaneRuntime, InstanceError> {
    let risk_limits = resolved_risk_limits(instance, risk_profiles_by_id)?;
    let target_order_notional_usd =
        resolved_target_order_notional_usd(instance, risk_profiles_by_id)?;
    let required_warmup_bars = required_warmup_bars(instance)?;
    let indicators = build_runtime_indicators(instance)?;
    let strategy = build_runtime_strategy(instance)?;

    Ok(LaneRuntime {
        config: instance.clone(),
        lane_symbol: symbol.to_owned(),
        execution_mode,
        state,
        resume_after_startup_reconcile,
        indicators,
        strategy,
        bar_builder: BarBuilder::new(symbol.to_owned(), instance.timeframe),
        risk_limits,
        target_order_notional_usd,
        inventory: inventory_state_from_runtime_fields(0.0, None, recovered_realized_pnl_usd),
        has_position: false,
        position_quantity: 0.0,
        position_notional_usd: 0.0,
        entry_price: None,
        realized_pnl_usd: recovered_realized_pnl_usd,
        daily_loss_pct_accumulated: 0.0,
        last_loss_reset_date: None,
        cooldown_until_ms: None,
        reconciliation_blocked: matches!(state, LaneRuntimeState::Reconciling),
        remote_net_qty: None,
        aggregate_managed_qty: 0.0,
        external_delta_qty: None,
        managed_remote_open_orders: 0,
        external_remote_open_orders: 0,
        warmup: InstanceWarmupState::new(required_warmup_bars),
        recovery_state: LaneRecoveryState::Healthy,
        recovery_started_at_ms: None,
        recovery_target_timestamp: None,
        recovery_last_progress_timestamp: None,
        recovery_last_error: None,
        recovery_consecutive_no_progress_cycles: 0,
        last_recovered_at_timestamp: None,
        last_dispatched_bar_timestamp: None,
        last_stream_update: None,
        connector_execution_constraints: None,
        connector_fractional_entry_supported: None,
        connector_execution_constraints_initialized: false,
    })
}

/// Builds all lane runtimes and the bot-to-lane catalog from config, recovered
/// snapshot state, and recovered realized `PnL`.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when a lane identity cannot
/// be resolved or a lane runtime cannot be built from config.
pub fn build_runtime_lanes(
    instances: &[InstanceConfig],
    account_modes: &HashMap<String, ExecutionMode, impl BuildHasher>,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
    snapshot_states: &HashMap<String, String, impl BuildHasher>,
    recovered_realized_pnl_by_lane: &HashMap<String, f64, impl BuildHasher>,
    default_start_paused_if_recovery_uncertain: bool,
) -> Result<RuntimeLaneBuild, InstanceError> {
    let mut runtimes = HashMap::new();
    let mut lanes_by_bot = HashMap::new();

    for instance in instances {
        let recovered = resolved_instance_state(
            instance,
            snapshot_states,
            default_start_paused_if_recovery_uncertain,
        );
        let execution_mode = account_modes
            .get(&instance.account)
            .copied()
            .unwrap_or(ExecutionMode::Paper);

        let mut lane_ids = Vec::with_capacity(instance.symbols.len());
        for symbol in &instance.symbols {
            let lane_id = lane_instance_id(instance, symbol)?;
            let recovered_realized_pnl_usd = recovered_realized_pnl_by_lane
                .get(&lane_id)
                .copied()
                .unwrap_or(0.0);
            let runtime = build_lane_runtime(
                instance,
                symbol,
                recovered.state,
                recovered.resume_after_startup_reconcile,
                execution_mode,
                risk_profiles_by_id,
                recovered_realized_pnl_usd,
            )?;
            lane_ids.push(lane_id.clone());
            runtimes.insert(lane_id, runtime);
        }

        lanes_by_bot.insert(instance.id.clone(), lane_ids);
    }

    Ok((runtimes, lanes_by_bot))
}
