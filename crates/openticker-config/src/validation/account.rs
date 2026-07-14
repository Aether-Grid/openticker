//! Account-level validation rules: connector kind, mode, secrets, budget,
//! cash-balance assets, and reconciliation settings.

use super::connectors::connector_capabilities;
use crate::error::ConfigError;
use crate::model::AccountConfig;
use openticker_core::ExecutionMode;
use std::collections::HashSet;

const BINANCE_ALLOWED_CASH_BALANCE_ASSETS: [&str; 5] = ["USD", "USDT", "USDC", "BUSD", "FDUSD"];

/// Validates that a referenced secret environment variable is set.
///
/// The error message deliberately names the env var (e.g.
/// `OPENTICKER_API_KEY`). The *name* is not a secret — it is public
/// configuration that an operator must know to fix the problem — whereas the
/// *value* is the secret. This function only checks `std::env::var(..).is_err()`
/// and never reads, stores, or interpolates the value, so no secret material
/// can leak into logs through this path. Naming the variable is the actionable
/// diagnostic an operator needs, so it is intentionally retained.
fn validate_secret_reference(secret_env: Option<&str>) -> Result<(), ConfigError> {
    let Some(secret_env) = secret_env else {
        return Ok(());
    };
    if secret_env.trim().is_empty() {
        return Err(ConfigError::validation(
            "secret environment variable name cannot be empty",
        ));
    }
    if std::env::var(secret_env).is_err() {
        return Err(ConfigError::validation(format!(
            "required secret env var `{secret_env}` is not set"
        )));
    }
    Ok(())
}

pub(super) fn validate_account_mode(account: &AccountConfig) -> Result<(), ConfigError> {
    if account.kind == "binance" && account.mode == ExecutionMode::Paper && !account.use_demo_mode {
        return Err(ConfigError::validation(format!(
            "account `{}` uses binance paper mode but `use_demo_mode` is false",
            account.id
        )));
    }

    Ok(())
}

pub(super) fn validate_account_connector_kind(account: &AccountConfig) -> Result<(), ConfigError> {
    if connector_capabilities(account.kind.as_str()).is_none() {
        return Err(ConfigError::validation(format!(
            "account `{}` has unsupported connector kind `{}`",
            account.id, account.kind
        )));
    }

    Ok(())
}

pub(super) fn validate_account_secret_requirements(
    account: &AccountConfig,
) -> Result<(), ConfigError> {
    let caps = connector_capabilities(account.kind.as_str()).ok_or_else(|| {
        ConfigError::validation(format!(
            "account `{}` has unsupported connector kind `{}`",
            account.id, account.kind
        ))
    })?;

    validate_account_secret_field(
        account,
        "api_key_env",
        account.api_key_env.as_deref(),
        caps.secrets.api_key,
    )?;
    validate_account_secret_field(
        account,
        "api_secret_env",
        account.api_secret_env.as_deref(),
        caps.secrets.api_secret,
    )?;
    validate_account_secret_field(
        account,
        "passphrase_env",
        account.passphrase_env.as_deref(),
        caps.secrets.passphrase,
    )?;

    Ok(())
}

pub(super) fn validate_account_total_budget(account: &AccountConfig) -> Result<(), ConfigError> {
    if !account.total_budget_usd.is_finite() || account.total_budget_usd <= 0.0 {
        return Err(ConfigError::validation(format!(
            "account `{}` has invalid `total_budget_usd` `{}`; must be a positive finite number",
            account.id, account.total_budget_usd
        )));
    }
    Ok(())
}

pub(super) fn validate_account_cash_balance_assets(
    account: &AccountConfig,
) -> Result<(), ConfigError> {
    if account.cash_balance_assets.is_empty() {
        return Ok(());
    }

    if account.kind != "binance" {
        return Err(ConfigError::validation(format!(
            "account `{}` kind `{}` does not support `cash_balance_assets`; currently only `binance` supports this setting",
            account.id, account.kind
        )));
    }

    let mut seen = HashSet::new();
    for raw_asset in &account.cash_balance_assets {
        let asset = raw_asset.trim().to_ascii_uppercase();
        if asset.is_empty() {
            return Err(ConfigError::validation(format!(
                "account `{}` has blank `cash_balance_assets` entries",
                account.id
            )));
        }
        if !seen.insert(asset.clone()) {
            return Err(ConfigError::validation(format!(
                "account `{}` has duplicate `cash_balance_assets` entry `{asset}`",
                account.id
            )));
        }
        if !BINANCE_ALLOWED_CASH_BALANCE_ASSETS.contains(&asset.as_str()) {
            return Err(ConfigError::validation(format!(
                "account `{}` has unsupported `cash_balance_assets` entry `{asset}`; allowed values for binance are: {}",
                account.id,
                BINANCE_ALLOWED_CASH_BALANCE_ASSETS.join(",")
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_account_reconciliation_settings(
    account: &AccountConfig,
) -> Result<(), ConfigError> {
    if account.reconciliation_base_url.is_some()
        && !account.reconciliation_remote_snapshot
        && !account.execution_remote_submission_enabled()
    {
        return Err(ConfigError::validation(format!(
            "account `{}` sets `reconciliation_base_url` but both `reconciliation_remote_snapshot` and `execution_remote_submission` are disabled",
            account.id
        )));
    }

    if let Some(base_url) = account.reconciliation_base_url.as_deref()
        && !base_url.starts_with("https://")
        && !base_url.starts_with("http://")
    {
        return Err(ConfigError::validation(format!(
            "account `{}` has invalid `reconciliation_base_url` `{base_url}`; expected http:// or https://",
            account.id
        )));
    }

    Ok(())
}

fn validate_account_secret_field(
    account: &AccountConfig,
    field: &str,
    secret_env: Option<&str>,
    required: bool,
) -> Result<(), ConfigError> {
    if required && secret_env.is_none() {
        return Err(ConfigError::validation(format!(
            "account `{}` kind `{}` requires `{field}`",
            account.id, account.kind
        )));
    }

    validate_secret_reference(secret_env)
}
