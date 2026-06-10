//! Reusable configuration reload pipeline.
//!
//! Owns change-set validation, the runtime swap sequence, and reload status
//! bookkeeping so manual API reloads, config write endpoints, and the file
//! watcher can all share one implementation.

use crate::handlers::unix_now_ms;
use crate::state::HttpState;
use openticker_config::{ConfigBundle, load_from_dir};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering;
use tokio::sync::MutexGuard;
use tracing::{info, warn};

/// Maximum number of [`ConfigReloadStatus`] entries retained in the ring buffer.
pub(crate) const RELOAD_STATUS_HISTORY_LIMIT: usize = 20;

/// What initiated a configuration reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReloadTrigger {
    /// `POST /v1/config/reload`.
    ManualApi,
    /// A config-mutating API endpoint (wired in by the config write endpoints).
    ConfigWrite,
    /// The config directory file watcher.
    #[allow(dead_code)]
    FileWatcher,
}

impl ReloadTrigger {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ManualApi => "manual_api",
            Self::ConfigWrite => "config_write",
            Self::FileWatcher => "file_watcher",
        }
    }
}

/// A single restart-required (or reconciliation-required) finding from
/// change-set validation.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ReloadViolation {
    /// Stable machine-readable code, e.g. `storage_changed`.
    pub code: &'static str,
    /// `"global"`, `"account:<id>"`, or `"bot:<id>"`.
    pub scope: String,
    /// Human-readable explanation.
    pub message: String,
}

/// One recorded reload attempt, regardless of outcome.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConfigReloadStatus {
    pub(crate) at_ms: i64,
    /// `"manual_api"`, `"config_write"`, or `"file_watcher"`.
    pub(crate) trigger: &'static str,
    /// `"reloaded"`, `"no_change"`, `"rejected"`, or `"failed"`.
    pub(crate) outcome: &'static str,
    pub(crate) error: Option<String>,
    pub(crate) violations: Vec<ReloadViolation>,
    pub(crate) generation: u64,
}

/// Why a configuration reload/apply attempt did not change the runtime.
#[derive(Debug)]
pub(crate) enum ConfigApplyError {
    /// The service was started without a managed config directory.
    NotManaged,
    /// Loading or validating the on-disk configuration failed.
    Load(openticker_config::ConfigError),
    /// Change-set validation rejected the new bundle.
    ChangeSet(Vec<ReloadViolation>),
    /// Rebuilding the runtime from the new bundle failed.
    RuntimeBuild(String),
}

/// Rebuilding the runtime from a new bundle failed.
///
/// Deliberately narrower than [`ConfigApplyError`] so the status recording in
/// [`apply_bundle_and_record`] stays exhaustive: adding a new failure mode to
/// [`apply_bundle`] is a compile error until it is recorded explicitly.
#[derive(Debug)]
pub(crate) struct RuntimeBuildError(pub(crate) String);

/// Joins violation messages into a single human-readable summary.
pub(crate) fn violation_summary(violations: &[ReloadViolation]) -> String {
    violations
        .iter()
        .map(|violation| violation.message.as_str())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Validates that the differences between `current` and `next` can be applied
/// without a restart, collecting every violation instead of stopping at the
/// first one.
pub(crate) fn validate_reload_change_set(
    current: &ConfigBundle,
    next: &ConfigBundle,
    running_instance_ids: &HashSet<String>,
) -> Vec<ReloadViolation> {
    let mut violations = Vec::new();

    if current.global.storage.kind != next.global.storage.kind
        || current.global.storage.path != next.global.storage.path
    {
        violations.push(ReloadViolation {
            code: "storage_changed",
            scope: "global".to_owned(),
            message: "reload requires restart because storage backend configuration changed"
                .to_owned(),
        });
    }

    if current.global.service.bot_dir != next.global.service.bot_dir {
        violations.push(ReloadViolation {
            code: "bot_dir_changed",
            scope: "global".to_owned(),
            message: "reload requires restart because service.bot_dir changed".to_owned(),
        });
    }

    collect_account_violations(current, next, &mut violations);
    collect_running_instance_violations(current, next, running_instance_ids, &mut violations);

    violations
}

fn collect_account_violations(
    current: &ConfigBundle,
    next: &ConfigBundle,
    violations: &mut Vec<ReloadViolation>,
) {
    let current_accounts = current
        .accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect::<HashMap<_, _>>();
    let next_accounts = next
        .accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect::<HashMap<_, _>>();

    let current_account_ids = current_accounts.keys().copied().collect::<HashSet<_>>();
    let next_account_ids = next_accounts.keys().copied().collect::<HashSet<_>>();
    if current_account_ids != next_account_ids {
        violations.push(ReloadViolation {
            code: "account_set_changed",
            scope: "global".to_owned(),
            message: "reload requires restart because account set changed".to_owned(),
        });
    }

    let mut shared_account_ids = current_account_ids
        .intersection(&next_account_ids)
        .copied()
        .collect::<Vec<_>>();
    shared_account_ids.sort_unstable();
    for account_id in shared_account_ids {
        let old_account = current_accounts[account_id];
        let new_account = next_accounts[account_id];

        if old_account.kind != new_account.kind
            || old_account.mode != new_account.mode
            || old_account.use_demo_mode != new_account.use_demo_mode
            || old_account.reconciliation_remote_snapshot
                != new_account.reconciliation_remote_snapshot
            || old_account.execution_remote_submission != new_account.execution_remote_submission
            || old_account.reconciliation_base_url != new_account.reconciliation_base_url
        {
            violations.push(ReloadViolation {
                code: "account_settings_changed",
                scope: format!("account:{account_id}"),
                message: format!(
                    "reload requires restart because connector settings changed for account `{account_id}`"
                ),
            });
        }

        if old_account.api_key_env != new_account.api_key_env
            || old_account.api_secret_env != new_account.api_secret_env
            || old_account.passphrase_env != new_account.passphrase_env
        {
            violations.push(ReloadViolation {
                code: "credentials_changed",
                scope: format!("account:{account_id}"),
                message: format!(
                    "reload requires restart because credential references changed for account `{account_id}`"
                ),
            });
        }
    }
}

fn collect_running_instance_violations(
    current: &ConfigBundle,
    next: &ConfigBundle,
    running_instance_ids: &HashSet<String>,
    violations: &mut Vec<ReloadViolation>,
) {
    let current_instances = current
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<HashMap<_, _>>();
    let next_instances = next
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<HashMap<_, _>>();

    let mut running_ids = running_instance_ids.iter().collect::<Vec<_>>();
    running_ids.sort_unstable();
    for instance_id in running_ids {
        let Some(old_instance) = current_instances.get(instance_id.as_str()) else {
            violations.push(ReloadViolation {
                code: "running_instance_missing",
                scope: format!("bot:{instance_id}"),
                message: format!(
                    "running instance `{instance_id}` is missing in current config bundle"
                ),
            });
            continue;
        };
        let Some(new_instance) = next_instances.get(instance_id.as_str()) else {
            violations.push(ReloadViolation {
                code: "running_instance_removed",
                scope: format!("bot:{instance_id}"),
                message: format!(
                    "reload requires restart because running instance `{instance_id}` was removed"
                ),
            });
            continue;
        };

        if old_instance.timeframe != new_instance.timeframe {
            violations.push(ReloadViolation {
                code: "timeframe_changed_running",
                scope: format!("bot:{instance_id}"),
                message: format!(
                    "reload requires restart because timeframe changed for running instance `{instance_id}`"
                ),
            });
        }

        if old_instance.symbols != new_instance.symbols {
            violations.push(ReloadViolation {
                code: "symbols_changed_running",
                scope: format!("bot:{instance_id}"),
                message: format!(
                    "reload requires reconciliation because symbols changed for running instance `{instance_id}`"
                ),
            });
        }

        if old_instance.account != new_instance.account
            || old_instance.data_connector != new_instance.data_connector
            || old_instance.execution_connector != new_instance.execution_connector
            || old_instance.market != new_instance.market
        {
            violations.push(ReloadViolation {
                code: "bindings_changed_running",
                scope: format!("bot:{instance_id}"),
                message: format!(
                    "reload requires restart because connector or account bindings changed for running instance `{instance_id}`"
                ),
            });
        }
    }
}

/// Swaps the live runtime over to `bundle`.
///
/// When `prebuilt_runtime` is `None`, the runtime is rebuilt from `bundle`;
/// passing `Some` lets callers (e.g. config write endpoints) build the runtime
/// before persisting anything to disk. The `_config_write_guard` parameter is
/// a structural proof that the caller holds `state.config_write_lock` for the
/// duration of the call.
pub(crate) async fn apply_bundle(
    state: &HttpState,
    bundle: ConfigBundle,
    prebuilt_runtime: Option<openticker_runtime::Runtime>,
    _config_write_guard: &MutexGuard<'_, ()>,
) -> Result<(), RuntimeBuildError> {
    let reloaded_runtime = match prebuilt_runtime {
        Some(runtime) => runtime,
        None => openticker_runtime::Runtime::from_config_with_storage(&bundle)
            .map_err(|error| RuntimeBuildError(error.to_string()))?,
    };
    let reloaded_query = reloaded_runtime.query_handle();

    {
        let mut runtime = state.runtime.write().await;
        *runtime = reloaded_runtime;
    }

    {
        let mut query = state.query.write().await;
        *query = reloaded_query;
    }

    {
        let runtime = state.runtime.read().await;
        state
            .data_plane
            .replace_streams(runtime.effective_streams_for_dataplane());
    }

    {
        let mut config_bundle = state.config_bundle.write().await;
        *config_bundle = Some(bundle);
    }

    Ok(())
}

/// Applies `bundle` via [`apply_bundle`] and performs the shared bookkeeping:
/// a failed apply is recorded as a `"failed"` status, a successful apply bumps
/// `state.reload_generation` and records a `"reloaded"` status.
///
/// Returns the new generation on success. This is the single place where the
/// bump-and-record dance lives, so reloads and config write endpoints cannot
/// diverge.
pub(crate) async fn apply_bundle_and_record(
    state: &HttpState,
    bundle: ConfigBundle,
    prebuilt_runtime: Option<openticker_runtime::Runtime>,
    trigger: ReloadTrigger,
    config_write_guard: &MutexGuard<'_, ()>,
) -> Result<u64, ConfigApplyError> {
    if let Err(RuntimeBuildError(message)) =
        apply_bundle(state, bundle, prebuilt_runtime, config_write_guard).await
    {
        warn!(error = %message, "config apply failed while rebuilding runtime");
        record_status(
            state,
            trigger,
            "failed",
            Some(format!("config reload failed: {message}")),
            Vec::new(),
        )
        .await;
        return Err(ConfigApplyError::RuntimeBuild(message));
    }

    Ok(record_applied(state, trigger).await)
}

/// Reloads configuration from the managed config directory.
///
/// Returns `Ok(true)` when a new bundle was applied, `Ok(false)` when the
/// on-disk configuration is identical to the active bundle (no-op). Every
/// reload outcome is recorded in `state.reload_status` (except
/// [`ConfigApplyError::NotManaged`], which is a static deployment property);
/// `state.reload_generation` is bumped only when a reload actually applied.
pub(crate) async fn reload_from_disk(
    state: &HttpState,
    trigger: ReloadTrigger,
) -> Result<bool, ConfigApplyError> {
    let Some(config_dir) = state.config_dir.clone() else {
        // Running without a managed config directory is a static deployment
        // property, not a reload outcome, so it is not recorded in the ring.
        warn!("config reload requested but service is not using a managed config directory");
        return Err(ConfigApplyError::NotManaged);
    };

    let write_guard = state.config_write_lock.lock().await;

    let bundle = match load_from_dir(&config_dir) {
        Ok(bundle) => bundle,
        Err(error) => {
            warn!(error = %error, "config reload failed while loading files");
            record_status(
                state,
                trigger,
                "failed",
                Some(format!("config reload failed: {error}")),
                Vec::new(),
            )
            .await;
            return Err(ConfigApplyError::Load(error));
        }
    };

    {
        let current_bundle = state.config_bundle.read().await;
        if let Some(current) = current_bundle.as_ref()
            && bundles_equal(current, &bundle)
        {
            info!("config reload detected no changes; runtime left untouched");
            record_status(state, trigger, "no_change", None, Vec::new()).await;
            return Ok(false);
        }
    }

    let running_instance_ids = {
        let runtime = state.runtime.read().await;
        runtime
            .list_instances()
            .into_iter()
            .filter(|instance| {
                matches!(
                    instance.state,
                    openticker_runtime::InstanceRuntimeState::Running
                )
            })
            .map(|instance| instance.id)
            .collect::<HashSet<_>>()
    };

    {
        let current_bundle = state.config_bundle.read().await;
        if let Some(current) = current_bundle.as_ref() {
            let violations = validate_reload_change_set(current, &bundle, &running_instance_ids);
            if !violations.is_empty() {
                let summary = violation_summary(&violations);
                warn!(error = %summary, "config reload rejected by change-set validation");
                record_status(
                    state,
                    trigger,
                    "rejected",
                    Some(format!("config reload failed: {summary}")),
                    violations.clone(),
                )
                .await;
                return Err(ConfigApplyError::ChangeSet(violations));
            }
        }
    }

    let generation = apply_bundle_and_record(state, bundle, None, trigger, &write_guard).await?;

    let instances = {
        let runtime = state.runtime.read().await;
        runtime.list_instances().len()
    };
    info!(instances, generation, "configuration reloaded successfully");
    Ok(true)
}

fn bundles_equal(current: &ConfigBundle, next: &ConfigBundle) -> bool {
    match (serde_json::to_value(current), serde_json::to_value(next)) {
        (Ok(current), Ok(next)) => current == next,
        _ => false,
    }
}

/// Records a non-applying reload outcome, reading the current generation
/// inside the `reload_status` critical section so the entry and the
/// generation it carries cannot skew.
async fn record_status(
    state: &HttpState,
    trigger: ReloadTrigger,
    outcome: &'static str,
    error: Option<String>,
    violations: Vec<ReloadViolation>,
) {
    let mut history = state.reload_status.write().await;
    let generation = state.reload_generation.load(Ordering::SeqCst);
    push_status(
        &mut history,
        ConfigReloadStatus {
            at_ms: unix_now_ms(),
            trigger: trigger.as_str(),
            outcome,
            error,
            violations,
            generation,
        },
    );
}

/// Bumps `state.reload_generation` and records the matching `"reloaded"`
/// entry inside a single `reload_status` critical section, so a reader
/// holding the history lock never observes a generation without its entry.
async fn record_applied(state: &HttpState, trigger: ReloadTrigger) -> u64 {
    let mut history = state.reload_status.write().await;
    let generation = state.reload_generation.fetch_add(1, Ordering::SeqCst) + 1;
    push_status(
        &mut history,
        ConfigReloadStatus {
            at_ms: unix_now_ms(),
            trigger: trigger.as_str(),
            outcome: "reloaded",
            error: None,
            violations: Vec::new(),
            generation,
        },
    );
    generation
}

fn push_status(history: &mut VecDeque<ConfigReloadStatus>, entry: ConfigReloadStatus) {
    while history.len() >= RELOAD_STATUS_HISTORY_LIMIT {
        history.pop_front();
    }
    history.push_back(entry);
}
