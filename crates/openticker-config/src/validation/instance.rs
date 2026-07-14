//! Instance-level validation rules: connector bindings, budgets, execution
//! constraints, signal-delivery mode, and per-account budget allocation.

use super::connectors::{
    connector_capabilities, connector_supports_market, connector_supports_preview_market_stream,
    market_type_label,
};
use crate::error::ConfigError;
use crate::model::{AccountConfig, InstanceConfig, SignalMode};
use openticker_core::ExecutionMode;
use std::collections::HashMap;

/// Validates an instance's budget percentage.
///
/// # Invariant
///
/// `budget.pct` is guaranteed finite (no NaN/Inf) and within `(0.0, 100.0]`
/// *at load time* by this check. This is enforced only here, at config load;
/// it is NOT re-checked on the hot path. Any runtime arithmetic that derives
/// allocations from `budget.pct` MUST preserve finiteness (e.g. avoid dividing
/// by a zero denominator or multiplying by a non-finite factor), because a
/// reintroduced NaN/Inf would not be caught again after load.
pub(super) fn validate_instance_budget(instance: &InstanceConfig) -> Result<(), ConfigError> {
    let pct = instance.budget.pct;
    if !pct.is_finite() || pct <= 0.0 {
        return Err(ConfigError::validation(format!(
            "instance `{}` has invalid `budget.pct` `{pct}`; must be a positive finite number (set `enabled = false` to disable a bot)",
            instance.id
        )));
    }
    if pct > 100.0 {
        return Err(ConfigError::validation(format!(
            "instance `{}` has `budget.pct` `{pct}` exceeding 100.0",
            instance.id
        )));
    }
    Ok(())
}

pub(super) fn validate_account_budget_allocations(
    instances: &[InstanceConfig],
    accounts: &[AccountConfig],
) -> Result<(), ConfigError> {
    let mut sums: HashMap<&str, (f64, Vec<&str>)> = HashMap::new();
    for instance in instances {
        if !instance.enabled {
            continue;
        }
        let entry = sums
            .entry(instance.account.as_str())
            .or_insert_with(|| (0.0, Vec::new()));
        entry.0 += instance.budget.pct;
        entry.1.push(instance.id.as_str());
    }
    for account in accounts {
        let (sum, bot_ids) = sums
            .get(account.id.as_str())
            .cloned()
            .unwrap_or_else(|| (0.0, Vec::new()));
        if sum > 100.0 + f64::EPSILON {
            return Err(ConfigError::validation(format!(
                "account `{}` has bots whose `budget.pct` sums to {sum:.4}%, exceeding 100%. Contributing bots: {}",
                account.id,
                bot_ids.join(", ")
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_instance_connector_bindings(
    instance: &InstanceConfig,
    account: &AccountConfig,
) -> Result<(), ConfigError> {
    let data_caps = connector_capabilities(instance.data_connector.as_str()).ok_or_else(|| {
        ConfigError::validation(format!(
            "instance `{}` has unsupported data_connector `{}`",
            instance.id, instance.data_connector
        ))
    })?;
    if !data_caps.roles.data {
        return Err(ConfigError::validation(format!(
            "instance `{}` data_connector `{}` does not support data role",
            instance.id, instance.data_connector
        )));
    }
    if !connector_supports_market(data_caps, instance.market) {
        return Err(ConfigError::validation(format!(
            "instance `{}` data_connector `{}` does not support market `{}`",
            instance.id,
            instance.data_connector,
            market_type_label(instance.market)
        )));
    }

    let execution_caps =
        connector_capabilities(instance.execution_connector.as_str()).ok_or_else(|| {
            ConfigError::validation(format!(
                "instance `{}` has unsupported execution_connector `{}`",
                instance.id, instance.execution_connector
            ))
        })?;
    if !execution_caps.roles.execution {
        return Err(ConfigError::validation(format!(
            "instance `{}` execution_connector `{}` does not support execution role",
            instance.id, instance.execution_connector
        )));
    }
    if !connector_supports_market(execution_caps, instance.market) {
        return Err(ConfigError::validation(format!(
            "instance `{}` execution_connector `{}` does not support market `{}`",
            instance.id,
            instance.execution_connector,
            market_type_label(instance.market)
        )));
    }

    if instance.execution_connector != account.kind {
        return Err(ConfigError::validation(format!(
            "instance `{}` execution_connector `{}` does not match account `{}` kind `{}`",
            instance.id, instance.execution_connector, account.id, account.kind
        )));
    }

    Ok(())
}

pub(super) fn validate_execution_constraints(instance: &InstanceConfig) -> Result<(), ConfigError> {
    if let Some(quantity_step) = instance.execution_constraints.quantity_step
        && (!quantity_step.is_finite() || quantity_step <= 0.0)
    {
        return Err(ConfigError::validation(format!(
            "instance `{}` has invalid `execution_constraints.quantity_step` `{quantity_step}`",
            instance.id
        )));
    }

    if let Some(min_quantity) = instance.execution_constraints.min_quantity
        && (!min_quantity.is_finite() || min_quantity <= 0.0)
    {
        return Err(ConfigError::validation(format!(
            "instance `{}` has invalid `execution_constraints.min_quantity` `{min_quantity}`",
            instance.id
        )));
    }

    if let Some(min_notional_usd) = instance.execution_constraints.min_notional_usd
        && (!min_notional_usd.is_finite() || min_notional_usd <= 0.0)
    {
        return Err(ConfigError::validation(format!(
            "instance `{}` has invalid `execution_constraints.min_notional_usd` `{min_notional_usd}`",
            instance.id
        )));
    }

    Ok(())
}

pub(super) fn validate_signal_delivery_mode(
    instance: &InstanceConfig,
    account: &AccountConfig,
) -> Result<(), ConfigError> {
    if !matches!(instance.signal_mode, SignalMode::Intrabar) {
        return Ok(());
    }

    if account.mode == ExecutionMode::Live {
        return Err(ConfigError::validation(format!(
            "instance `{}` cannot use signal_mode `intrabar` with live account `{}`; live intrabar trading is hard-rejected until parity is proven",
            instance.id, instance.account
        )));
    }

    if !connector_supports_preview_market_stream(instance.data_connector.as_str()) {
        return Err(ConfigError::validation(format!(
            "instance `{}` signal_mode `intrabar` requires data_connector `{}` to support preview market-stream bars",
            instance.id, instance.data_connector
        )));
    }

    Ok(())
}
