use crate::error::ConfigError;
use crate::model::{
    AccountConfig, AccountSecretStatus, ConfigBundle, DataPlaneConfig, EffectiveAccountConfig,
    EffectiveConfig, InstanceConfig, SignalMode, StorageConfig,
};
use openticker_core::{
    ExecutionMode, IndicatorRole, IndicatorSignalPolicy, IndicatorStabilityClass, MarketType,
};
use openticker_signals::indicator_manifest;
use std::collections::{HashMap, HashSet};

const BINANCE_ALLOWED_CASH_BALANCE_ASSETS: [&str; 5] = ["USD", "USDT", "USDC", "BUSD", "FDUSD"];

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

    #[must_use]
    pub fn effective_config(&self) -> EffectiveConfig {
        let accounts = self
            .accounts
            .iter()
            .map(|account| EffectiveAccountConfig {
                id: account.id.clone(),
                kind: account.kind.clone(),
                mode: account.mode,
                use_demo_mode: account.use_demo_mode,
                reconciliation_remote_snapshot: account.reconciliation_remote_snapshot,
                execution_remote_submission: account.execution_remote_submission_enabled(),
                reconciliation_base_url: account.reconciliation_base_url.clone(),
                cash_balance_assets: account.cash_balance_assets.clone(),
                total_budget_usd: account.total_budget_usd,
                secret_status: AccountSecretStatus {
                    api_key_present: secret_present(account.api_key_env.as_deref()),
                    api_secret_present: secret_present(account.api_secret_env.as_deref()),
                    passphrase_present: secret_present(account.passphrase_env.as_deref()),
                },
            })
            .collect();

        EffectiveConfig {
            global: self.global.clone(),
            accounts,
            risk_profiles: self.risk_profiles.clone(),
            instances: self.instances.clone(),
        }
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

fn validate_storage(storage: &StorageConfig) -> Result<(), ConfigError> {
    if storage.kind != "sqlite" {
        return Err(ConfigError::validation(format!(
            "unsupported storage kind `{}`; only `sqlite` is currently supported",
            storage.kind
        )));
    }

    if storage.path.as_os_str().is_empty() {
        return Err(ConfigError::validation(
            "storage path cannot be empty for sqlite backend",
        ));
    }

    if storage.busy_timeout_ms == 0 {
        return Err(ConfigError::validation(
            "storage busy_timeout_ms must be greater than zero",
        ));
    }

    Ok(())
}

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

fn validate_account_mode(account: &AccountConfig) -> Result<(), ConfigError> {
    if account.kind == "binance" && account.mode == ExecutionMode::Paper && !account.use_demo_mode {
        return Err(ConfigError::validation(format!(
            "account `{}` uses binance paper mode but `use_demo_mode` is false",
            account.id
        )));
    }

    Ok(())
}

fn validate_account_connector_kind(account: &AccountConfig) -> Result<(), ConfigError> {
    if connector_capabilities(account.kind.as_str()).is_none() {
        return Err(ConfigError::validation(format!(
            "account `{}` has unsupported connector kind `{}`",
            account.id, account.kind
        )));
    }

    Ok(())
}

fn validate_account_secret_requirements(account: &AccountConfig) -> Result<(), ConfigError> {
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

fn validate_account_total_budget(account: &AccountConfig) -> Result<(), ConfigError> {
    if !account.total_budget_usd.is_finite() || account.total_budget_usd <= 0.0 {
        return Err(ConfigError::validation(format!(
            "account `{}` has invalid `total_budget_usd` `{}`; must be a positive finite number",
            account.id, account.total_budget_usd
        )));
    }
    Ok(())
}

fn validate_account_cash_balance_assets(account: &AccountConfig) -> Result<(), ConfigError> {
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

fn normalized_symbol_key(symbol: &str) -> String {
    symbol.trim().to_ascii_uppercase()
}

fn validate_instance_budget(instance: &InstanceConfig) -> Result<(), ConfigError> {
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

fn validate_account_budget_allocations(
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

fn validate_account_reconciliation_settings(account: &AccountConfig) -> Result<(), ConfigError> {
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

fn validate_instance_connector_bindings(
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

fn validate_execution_constraints(instance: &InstanceConfig) -> Result<(), ConfigError> {
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

#[derive(Debug, Clone, Copy)]
struct ConnectorCapabilities {
    roles: ConnectorRoleCapabilities,
    markets: ConnectorMarketCapabilities,
    secrets: ConnectorSecretRequirements,
}

#[derive(Debug, Clone, Copy)]
struct ConnectorRoleCapabilities {
    data: bool,
    execution: bool,
}

#[derive(Debug, Clone, Copy)]
struct ConnectorMarketCapabilities {
    equities: bool,
    crypto: bool,
}

#[derive(Debug, Clone, Copy)]
struct ConnectorSecretRequirements {
    api_key: bool,
    api_secret: bool,
    passphrase: bool,
}

fn connector_capabilities(kind: &str) -> Option<ConnectorCapabilities> {
    match kind {
        "alpaca" => Some(ConnectorCapabilities {
            roles: ConnectorRoleCapabilities {
                data: true,
                execution: true,
            },
            markets: ConnectorMarketCapabilities {
                equities: true,
                crypto: false,
            },
            secrets: ConnectorSecretRequirements {
                api_key: true,
                api_secret: true,
                passphrase: false,
            },
        }),
        "binance" => Some(ConnectorCapabilities {
            roles: ConnectorRoleCapabilities {
                data: true,
                execution: true,
            },
            markets: ConnectorMarketCapabilities {
                equities: false,
                crypto: true,
            },
            secrets: ConnectorSecretRequirements {
                api_key: true,
                api_secret: true,
                passphrase: false,
            },
        }),
        _ => None,
    }
}

fn connector_supports_market(caps: ConnectorCapabilities, market: MarketType) -> bool {
    match market {
        MarketType::Equities => caps.markets.equities,
        MarketType::Crypto => caps.markets.crypto,
    }
}

fn connector_supports_preview_market_stream(kind: &str) -> bool {
    matches!(kind, "binance")
}

fn market_type_label(market: MarketType) -> &'static str {
    match market {
        MarketType::Equities => "equities",
        MarketType::Crypto => "crypto",
    }
}

fn validate_signal_delivery_mode(
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

#[allow(clippy::too_many_lines)]
fn validate_indicators(instance: &InstanceConfig, live_account: bool) -> Result<(), ConfigError> {
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

fn secret_present(secret_env: Option<&str>) -> bool {
    secret_env
        .and_then(|name| std::env::var(name).ok())
        .is_some_and(|value| !value.is_empty())
}

fn validate_data_plane(
    data_plane: &DataPlaneConfig,
    account_ids: &HashSet<&str>,
) -> Result<(), ConfigError> {
    if data_plane.default_polling_interval_ms < 1_000 {
        return Err(ConfigError::validation(format!(
            "`data_plane.default_polling_interval_ms` must be at least 1000, got {}",
            data_plane.default_polling_interval_ms
        )));
    }

    if data_plane.default_retention < 10 {
        return Err(ConfigError::validation(format!(
            "`data_plane.default_retention` must be at least 10, got {}",
            data_plane.default_retention
        )));
    }

    for watch in &data_plane.watchlist {
        if !account_ids.contains(watch.account.as_str()) {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry references unknown account `{}`",
                watch.account
            )));
        }

        let polling_interval_ms = watch
            .polling_interval_ms
            .unwrap_or(data_plane.default_polling_interval_ms);
        if polling_interval_ms < 1_000 {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry `{}/{}` has invalid `polling_interval_ms` `{polling_interval_ms}`",
                watch.account, watch.symbol
            )));
        }

        let retention = watch.retention.unwrap_or(data_plane.default_retention);
        if retention < 10 {
            return Err(ConfigError::validation(format!(
                "data-plane watchlist entry `{}/{}` has invalid `retention` `{retention}`",
                watch.account, watch.symbol
            )));
        }
    }

    Ok(())
}
