//! Redacted "effective configuration" projection safe for external exposure.

use crate::model::{ConfigBundle, GlobalConfig, InstanceConfig, RiskProfileConfig};
use openticker_core::ExecutionMode;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveConfig {
    pub global: GlobalConfig,
    pub accounts: Vec<EffectiveAccountConfig>,
    pub risk_profiles: Vec<RiskProfileConfig>,
    #[serde(rename = "bots")]
    pub instances: Vec<InstanceConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveAccountConfig {
    pub id: String,
    pub kind: String,
    pub mode: ExecutionMode,
    pub use_demo_mode: bool,
    pub reconciliation_remote_snapshot: bool,
    pub execution_remote_submission: bool,
    pub reconciliation_base_url: Option<String>,
    pub cash_balance_assets: Vec<String>,
    pub total_budget_usd: f64,
    pub secret_status: AccountSecretStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSecretStatus {
    pub api_key_present: bool,
    pub api_secret_present: bool,
    pub passphrase_present: bool,
}

impl ConfigBundle {
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

fn secret_present(secret_env: Option<&str>) -> bool {
    secret_env
        .and_then(|name| std::env::var(name).ok())
        .is_some_and(|value| !value.is_empty())
}
