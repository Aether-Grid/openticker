use openticker_core::{
    ExecutionMode, IndicatorRole, IndicatorSignalMetadataFilters, IndicatorSignalPolicy,
    MarketType, Timeframe,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub service: ServiceConfig,
    pub http: HttpConfig,
    pub storage: StorageConfig,
    pub observability: ObservabilityConfig,
    pub safety: SafetyConfig,
    #[serde(default)]
    pub data_plane: DataPlaneConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub environment: String,
    pub data_dir: PathBuf,
    pub bot_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    pub enabled: bool,
    pub bind: String,
    pub request_log: bool,
    pub openapi_enabled: bool,
    pub openapi_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub kind: String,
    pub path: PathBuf,
    pub busy_timeout_ms: u64,
    #[serde(default)]
    pub prune_removed_bots_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub metrics_enabled: bool,
    pub metrics_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub require_explicit_live_enable: bool,
    pub default_start_paused_if_recovery_uncertain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPlaneConfig {
    #[serde(default = "default_data_plane_polling_interval_ms")]
    pub default_polling_interval_ms: u64,
    #[serde(default = "default_data_plane_retention")]
    pub default_retention: usize,
    #[serde(default)]
    pub watchlist: Vec<DataPlaneWatchlistEntry>,
}

impl Default for DataPlaneConfig {
    fn default() -> Self {
        Self {
            default_polling_interval_ms: default_data_plane_polling_interval_ms(),
            default_retention: default_data_plane_retention(),
            watchlist: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPlaneWatchlistEntry {
    pub account: String,
    pub symbol: String,
    pub timeframe: Timeframe,
    #[serde(default)]
    pub polling_interval_ms: Option<u64>,
    #[serde(default)]
    pub retention: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub kind: String,
    pub mode: ExecutionMode,
    pub api_key_env: Option<String>,
    pub api_secret_env: Option<String>,
    pub passphrase_env: Option<String>,
    #[serde(default)]
    pub use_demo_mode: bool,
    #[serde(default)]
    pub reconciliation_remote_snapshot: bool,
    #[serde(default)]
    pub execution_remote_submission: Option<bool>,
    pub reconciliation_base_url: Option<String>,
    #[serde(default)]
    pub cash_balance_assets: Vec<String>,
    pub total_budget_usd: f64,
}

/// An API-facing account update payload: every [`AccountConfig`] field except
/// `id` and the secret env references (`api_key_env`, `api_secret_env`,
/// `passphrase_env`), which are immutable through the HTTP surface.
///
/// `deny_unknown_fields` makes any attempt to submit those excluded fields a
/// deserialization error, so secret references can never be changed (or
/// echoed back) through account update requests.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountConfigUpdate {
    pub kind: String,
    pub mode: ExecutionMode,
    #[serde(default)]
    pub use_demo_mode: bool,
    #[serde(default)]
    pub reconciliation_remote_snapshot: bool,
    #[serde(default)]
    pub execution_remote_submission: Option<bool>,
    #[serde(default)]
    pub reconciliation_base_url: Option<String>,
    #[serde(default)]
    pub cash_balance_assets: Vec<String>,
    pub total_budget_usd: f64,
}

impl AccountConfigUpdate {
    /// Builds a full [`AccountConfig`] from this update, copying the `id` and
    /// the three secret env references from `existing` untouched.
    #[must_use]
    pub fn into_account(self, existing: &AccountConfig) -> AccountConfig {
        AccountConfig {
            id: existing.id.clone(),
            kind: self.kind,
            mode: self.mode,
            api_key_env: existing.api_key_env.clone(),
            api_secret_env: existing.api_secret_env.clone(),
            passphrase_env: existing.passphrase_env.clone(),
            use_demo_mode: self.use_demo_mode,
            reconciliation_remote_snapshot: self.reconciliation_remote_snapshot,
            execution_remote_submission: self.execution_remote_submission,
            reconciliation_base_url: self.reconciliation_base_url,
            cash_balance_assets: self.cash_balance_assets,
            total_budget_usd: self.total_budget_usd,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskProfileConfig {
    pub id: String,
    pub max_daily_loss_pct: f64,
    pub max_open_positions: u32,
    #[serde(default)]
    pub target_order_notional_usd: Option<f64>,
    pub max_order_notional_usd: f64,
    pub max_spread_bps: u32,
    pub max_slippage_bps: u32,
    pub stale_data_ms: u64,
    pub cooldown_after_reject_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub id: String,
    pub enabled: bool,
    pub market: MarketType,
    pub symbols: Vec<String>,
    pub timeframe: Timeframe,
    pub account: String,
    pub data_connector: String,
    pub execution_connector: String,
    pub strategy: String,
    pub signal_mode: SignalMode,
    #[serde(default = "default_instance_polling_enabled")]
    pub polling_enabled: bool,
    #[serde(default = "default_instance_polling_interval_ms")]
    pub polling_interval_ms: u64,
    #[serde(default)]
    pub indicators: Vec<IndicatorInstanceConfig>,
    #[serde(default)]
    pub execution_constraints: ExecutionConstraintsConfig,
    pub budget: BudgetConfig,
    pub risk: InstanceRiskConfig,
    #[serde(default)]
    pub warmup_target_bars: Option<usize>,
    #[serde(default)]
    pub allow_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub pct: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionConstraintsConfig {
    pub quantity_step: Option<f64>,
    pub min_quantity: Option<f64>,
    pub min_notional_usd: Option<f64>,
}

const fn default_instance_polling_enabled() -> bool {
    true
}

const fn default_instance_polling_interval_ms() -> u64 {
    1_000
}

const fn default_data_plane_polling_interval_ms() -> u64 {
    5_000
}

const fn default_data_plane_retention() -> usize {
    500
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalMode {
    Intrabar,
    ConfirmedOnly,
}

const fn default_indicator_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorInstanceConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub indicator_type: String,
    #[serde(default = "default_indicator_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub role: Option<IndicatorRole>,
    #[serde(default)]
    pub signal_policy: Option<IndicatorSignalPolicy>,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub metadata_filters: IndicatorSignalMetadataFilters,
    #[serde(default)]
    pub params: toml::Table,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceRiskConfig {
    pub profile: String,
    #[serde(default)]
    pub overrides: RiskOverrides,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskOverrides {
    pub max_daily_loss_pct: Option<f64>,
    pub max_open_positions: Option<u32>,
    pub target_order_notional_usd: Option<f64>,
    pub max_order_notional_usd: Option<f64>,
    pub max_spread_bps: Option<u32>,
    pub max_slippage_bps: Option<u32>,
    pub stale_data_ms: Option<u64>,
    pub cooldown_after_reject_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigBundle {
    pub global: GlobalConfig,
    pub accounts: Vec<AccountConfig>,
    pub risk_profiles: Vec<RiskProfileConfig>,
    pub instances: Vec<InstanceConfig>,
}

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

impl AccountConfig {
    #[must_use]
    pub fn execution_remote_submission_enabled(&self) -> bool {
        self.execution_remote_submission
            .unwrap_or(self.reconciliation_remote_snapshot)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSecretStatus {
    pub api_key_present: bool,
    pub api_secret_present: bool,
    pub passphrase_present: bool,
}
