mod account;
mod connectors;
mod global;
mod indicators;
mod instance;

use crate::error::ConfigError;
use crate::model::{AccountConfig, ConfigBundle};
use account::{
    validate_account_cash_balance_assets, validate_account_connector_kind, validate_account_mode,
    validate_account_reconciliation_settings, validate_account_secret_requirements,
    validate_account_total_budget,
};
use global::{validate_data_plane, validate_storage};
use indicators::validate_indicators;
use instance::{
    validate_account_budget_allocations, validate_execution_constraints, validate_instance_budget,
    validate_instance_connector_bindings, validate_signal_delivery_mode,
};
use openticker_core::ExecutionMode;
use std::collections::{HashMap, HashSet};

impl ConfigBundle {
    /// Validates cross-file and semantic configuration invariants.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Validation`] when one or more configuration values are invalid,
    /// inconsistent, or reference missing resources.
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_storage(&self.global.storage)?;

        validate_unique_ids(
            self.accounts.iter().map(|account| account.id.as_str()),
            "account",
        )?;
        validate_unique_ids(
            self.risk_profiles.iter().map(|risk| risk.id.as_str()),
            "risk profile",
        )?;
        validate_unique_ids(
            self.instances.iter().map(|instance| instance.id.as_str()),
            "instance",
        )?;

        let account_ids: HashSet<&str> = self
            .accounts
            .iter()
            .map(|account| account.id.as_str())
            .collect();
        let accounts_by_id: HashMap<&str, &AccountConfig> = self
            .accounts
            .iter()
            .map(|account| (account.id.as_str(), account))
            .collect();
        let risk_ids: HashSet<&str> = self
            .risk_profiles
            .iter()
            .map(|risk| risk.id.as_str())
            .collect();

        validate_data_plane(&self.global.data_plane, &account_ids)?;

        for account in &self.accounts {
            validate_account_connector_kind(account)?;
            validate_account_mode(account)?;
            validate_account_secret_requirements(account)?;
            validate_account_reconciliation_settings(account)?;
            validate_account_cash_balance_assets(account)?;
            validate_account_total_budget(account)?;
        }

        for risk in &self.risk_profiles {
            if let Some(target_order_notional_usd) = risk.target_order_notional_usd
                && (!target_order_notional_usd.is_finite() || target_order_notional_usd <= 0.0)
            {
                return Err(ConfigError::validation(format!(
                    "risk profile `{}` has invalid `target_order_notional_usd` `{target_order_notional_usd}`",
                    risk.id
                )));
            }
        }

        for instance in &self.instances {
            if instance.id.trim().is_empty() {
                return Err(ConfigError::validation("instance id cannot be empty"));
            }
            if instance.symbols.is_empty() {
                return Err(ConfigError::validation(format!(
                    "instance `{}` must have at least one symbol",
                    instance.id
                )));
            }
            let mut normalized_symbols = HashSet::new();
            for symbol in &instance.symbols {
                let normalized_symbol = normalized_symbol_key(symbol);
                if normalized_symbol.is_empty() {
                    return Err(ConfigError::validation(format!(
                        "instance `{}` has a blank symbol entry",
                        instance.id
                    )));
                }
                if !normalized_symbols.insert(normalized_symbol.clone()) {
                    return Err(ConfigError::validation(format!(
                        "instance `{}` has duplicate symbol `{}` after normalization",
                        instance.id, normalized_symbol
                    )));
                }
            }
            if !account_ids.contains(instance.account.as_str()) {
                return Err(ConfigError::validation(format!(
                    "instance `{}` references unknown account `{}`",
                    instance.id, instance.account
                )));
            }
            let Some(account) = accounts_by_id.get(instance.account.as_str()).copied() else {
                return Err(ConfigError::validation(format!(
                    "instance `{}` references unknown account `{}`",
                    instance.id, instance.account
                )));
            };

            validate_instance_connector_bindings(instance, account)?;
            if !risk_ids.contains(instance.risk.profile.as_str()) {
                return Err(ConfigError::validation(format!(
                    "instance `{}` references unknown risk profile `{}`",
                    instance.id, instance.risk.profile
                )));
            }
            if instance.indicators.is_empty() {
                return Err(ConfigError::validation(format!(
                    "instance `{}` must define at least one indicator",
                    instance.id
                )));
            }
            if instance.polling_interval_ms == 0 {
                return Err(ConfigError::validation(format!(
                    "instance `{}` has invalid `polling_interval_ms` of 0",
                    instance.id
                )));
            }

            if let Some(target_order_notional_usd) =
                instance.risk.overrides.target_order_notional_usd
                && (!target_order_notional_usd.is_finite() || target_order_notional_usd <= 0.0)
            {
                return Err(ConfigError::validation(format!(
                    "instance `{}` has invalid `risk.overrides.target_order_notional_usd` `{target_order_notional_usd}`",
                    instance.id
                )));
            }

            validate_execution_constraints(instance)?;
            validate_instance_budget(instance)?;

            validate_signal_delivery_mode(instance, account)?;
            validate_indicators(instance, account.mode == ExecutionMode::Live)?;

            if self.global.safety.require_explicit_live_enable
                && account.mode == ExecutionMode::Live
                && !instance.allow_live
            {
                return Err(ConfigError::validation(format!(
                    "instance `{}` uses live account `{}` but `allow_live` is false",
                    instance.id, instance.account
                )));
            }
        }

        validate_account_budget_allocations(&self.instances, &self.accounts)?;

        Ok(())
    }
}

fn validate_unique_ids<'a>(
    ids: impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<(), ConfigError> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.trim().is_empty() {
            return Err(ConfigError::validation(format!(
                "{label} id cannot be empty"
            )));
        }
        if !seen.insert(id.to_owned()) {
            return Err(ConfigError::validation(format!(
                "duplicate {label} id `{id}`"
            )));
        }
    }
    Ok(())
}

fn normalized_symbol_key(symbol: &str) -> String {
    symbol.trim().to_ascii_uppercase()
}
