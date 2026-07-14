use crate::repo::bootstrap_journal_state;
use crate::{
    AccountLedger, ConfigBundle, ExecutionMode, InMemoryRuntimeJournal, OperatorReadModels,
    RiskLimits, RiskProfileRuntimeConfig, Runtime, RuntimeCatalog, RuntimeJournal, RuntimeState,
    ServiceError, ServiceObservability, SqliteRuntimeJournal, StorageError,
};
use openticker_gateway::build_connector_registry;
use openticker_lane::build_runtime_lanes;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tracing::info;

impl Runtime {
    /// Builds an in-memory runtime from a validated configuration bundle.
    ///
    /// # Panics
    ///
    /// Panics only if in-memory runtime initialization fails unexpectedly.
    #[must_use]
    pub fn from_config(config: &ConfigBundle) -> Self {
        let journal = Arc::new(InMemoryRuntimeJournal::default());
        Self::from_config_with_journal(config, journal)
            .expect("in-memory runtime initialization should not fail")
    }

    /// Builds a runtime backed by the configured persistent storage backend.
    ///
    /// When `storage.prune_removed_bots_on_startup` is enabled, journal
    /// history of bots absent from `config` is pruned before the runtime is
    /// bootstrapped.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration is invalid or storage initialization fails.
    pub fn from_config_with_storage(config: &ConfigBundle) -> Result<Self, ServiceError> {
        Self::from_config_with_storage_impl(config, true)
    }

    /// Like [`Self::from_config_with_storage`], but never runs the startup
    /// prune step, even when `storage.prune_removed_bots_on_startup` is
    /// enabled.
    ///
    /// Intended for callers that build a runtime from a *candidate*
    /// configuration before that configuration has been committed to disk
    /// (e.g. the HTTP config write endpoints): pruning at that point would
    /// destroy a removed bot's journal history (including `PnL` recovery data)
    /// even if the disk commit later fails, leaving a still-configured bot
    /// with destroyed history. With this variant, pruning of removed bots is
    /// deferred to the next service startup.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration is invalid or storage initialization fails.
    pub fn from_config_with_storage_without_prune(
        config: &ConfigBundle,
    ) -> Result<Self, ServiceError> {
        Self::from_config_with_storage_impl(config, false)
    }

    fn from_config_with_storage_impl(
        config: &ConfigBundle,
        allow_prune: bool,
    ) -> Result<Self, ServiceError> {
        config
            .validate()
            .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;

        let storage = &config.global.storage;
        info!(
            storage.kind = %storage.kind,
            storage.path = %storage.path.display(),
            "initializing openticker-runtime with persistent storage"
        );
        let journal: Arc<dyn RuntimeJournal> = match storage.kind.as_str() {
            "sqlite" => Arc::new(SqliteRuntimeJournal::open(
                &storage.path,
                storage.busy_timeout_ms,
            )?),
            unsupported => {
                return Err(ServiceError::Storage(StorageError::UnsupportedBackend {
                    kind: unsupported.to_owned(),
                }));
            }
        };

        if allow_prune && storage.prune_removed_bots_on_startup {
            let configured_instance_ids = config
                .instances
                .iter()
                .map(|instance| instance.id.clone())
                .collect::<HashSet<_>>();
            journal.prune_bots_except(&configured_instance_ids)?;
        }

        Self::from_config_with_journal(config, journal)
    }

    pub(crate) fn from_config_with_journal(
        config: &ConfigBundle,
        journal: Arc<dyn RuntimeJournal>,
    ) -> Result<Self, ServiceError> {
        let accounts = config
            .accounts
            .iter()
            .cloned()
            .map(|account| (account.id.clone(), account))
            .collect::<HashMap<_, _>>();
        let account_ledgers = Self::build_account_ledgers(config);

        let account_modes = Self::account_modes(config);
        let risk_profiles_by_id = Self::risk_profiles_by_id(config);
        let bootstrap = bootstrap_journal_state(config.instances.as_slice(), journal.as_ref())?;
        let (lanes, lanes_by_bot) = build_runtime_lanes(
            &config.instances,
            &account_modes,
            &risk_profiles_by_id,
            &bootstrap.snapshot_states,
            &bootstrap.recovered_realized_pnl_by_lane,
            config
                .global
                .safety
                .default_start_paused_if_recovery_uncertain,
        )
        .map_err(ServiceError::from)?;

        let connectors = Arc::new(Mutex::new(
            build_connector_registry(&config.accounts)
                .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?,
        ));
        let read_models = Arc::new(OperatorReadModels::bootstrap(journal.as_ref())?);

        let runtime = Self {
            state: RuntimeState {
                lanes,
                kill_switch_active: false,
                observability: ServiceObservability::default(),
            },
            catalog: RuntimeCatalog {
                lanes_by_bot,
                accounts,
                account_ledgers,
                data_plane_config: config.global.data_plane.clone(),
            },
            connectors,
            journal,
            read_models,
        };
        let mut runtime = runtime;
        runtime.repo().sync_all_account_ledgers_from_instances()?;
        runtime.refresh_all_account_ledgers_from_connectors()?;
        info!(
            accounts = runtime.catalog.accounts.len(),
            lanes = runtime.state.lanes.len(),
            "openticker-runtime initialized"
        );
        runtime.persist_boot_state()?;
        runtime.run_startup_reconciliation()?;
        runtime.run_startup_warmup();
        info!(
            ready = runtime.is_ready(),
            "openticker-runtime startup reconciliation complete"
        );
        Ok(runtime)
    }

    pub(crate) fn account_modes(config: &ConfigBundle) -> HashMap<String, ExecutionMode> {
        config
            .accounts
            .iter()
            .map(|account| (account.id.clone(), account.mode))
            .collect::<HashMap<_, _>>()
    }

    pub(crate) fn build_account_ledgers(
        config: &ConfigBundle,
    ) -> HashMap<String, Arc<Mutex<AccountLedger>>> {
        config
            .accounts
            .iter()
            .map(|account| {
                (
                    account.id.clone(),
                    Arc::new(Mutex::new(AccountLedger::new(account.total_budget_usd))),
                )
            })
            .collect::<HashMap<_, _>>()
    }

    pub(crate) fn risk_profiles_by_id(
        config: &ConfigBundle,
    ) -> HashMap<String, RiskProfileRuntimeConfig> {
        config
            .risk_profiles
            .iter()
            .map(|profile| {
                let target_order_notional_usd = profile
                    .target_order_notional_usd
                    .unwrap_or(profile.max_order_notional_usd);
                (
                    profile.id.clone(),
                    RiskProfileRuntimeConfig {
                        limits: RiskLimits {
                            max_daily_loss_pct: profile.max_daily_loss_pct,
                            max_open_positions: profile.max_open_positions,
                            max_order_notional_usd: profile.max_order_notional_usd,
                            max_spread_bps: profile.max_spread_bps,
                            max_slippage_bps: profile.max_slippage_bps,
                            stale_data_ms: profile.stale_data_ms,
                            cooldown_after_reject_ms: profile.cooldown_after_reject_ms,
                        },
                        target_order_notional_usd,
                    },
                )
            })
            .collect::<HashMap<_, _>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_temp_db_path, fixture_bundle, fixture_bundle_with_db_path};
    use crate::{BotSnapshotWrite, EventWrite, LaneRuntimeState, OrderWrite, SignalWrite};

    #[test]
    fn bootstrap_rejects_invalid_connector_bindings() {
        let db_path = create_temp_db_path("bootstrap-invalid-config");
        let mut config = fixture_bundle_with_db_path(db_path);
        config.instances[0].execution_connector = "kraken".to_owned();

        let result = Runtime::from_config_with_storage(&config);
        assert!(matches!(
            result,
            Err(ServiceError::InvalidConfiguration(message))
                if message.contains("execution_connector")
        ));
    }

    #[test]
    fn bootstrap_rejects_missing_required_connector_secrets() {
        let db_path = create_temp_db_path("bootstrap-missing-secrets");
        let mut config = fixture_bundle_with_db_path(db_path);
        config.accounts[0].api_secret_env = None;

        let result = Runtime::from_config_with_storage(&config);
        assert!(matches!(
            result,
            Err(ServiceError::InvalidConfiguration(message))
                if message.contains("requires `api_secret_env`")
        ));
    }

    #[test]
    fn startup_opt_in_prunes_removed_bot_history() {
        let db_path = create_temp_db_path("startup-prune-removed-bots");
        let mut config = fixture_bundle_with_db_path(db_path.clone());
        config.global.storage.prune_removed_bots_on_startup = true;

        let journal =
            SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms).unwrap();
        journal
            .append_event(EventWrite {
                scope: "instance".to_owned(),
                entity_id: Some("msft".to_owned()),
                trace_id: None,
                kind: "instance.started".to_owned(),
                payload: "{}".to_owned(),
            })
            .unwrap();
        journal
            .append_signal(SignalWrite {
                bot_id: "msft".to_owned(),
                symbol: "MSFT".to_owned(),
                trace_id: None,
                bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
                phase: "confirmed".to_owned(),
                signal: "buy_confirmed".to_owned(),
                close: 100.0,
                metadata_json: None,
            })
            .unwrap();
        journal
            .append_order(OrderWrite {
                bot_id: "msft".to_owned(),
                symbol: "MSFT".to_owned(),
                trace_id: None,
                bar_timestamp: None,
                client_order_id: "msft-1-open_long".to_owned(),
                intent: "open_long".to_owned(),
                status: "submitted".to_owned(),
                price: 100.0,
                quantity: 1.0,
            })
            .unwrap();
        journal
            .upsert_bot_snapshot(BotSnapshotWrite {
                bot_id: "msft".to_owned(),
                state: "running".to_owned(),
                execution_mode: "paper".to_owned(),
                enabled: true,
            })
            .unwrap();

        let runtime = Runtime::from_config_with_storage(&config).unwrap();
        assert!(
            runtime
                .recent_events_for_entity("msft", 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            runtime
                .recent_orders_for_bot("msft", 10)
                .unwrap()
                .is_empty()
        );

        let reopened =
            SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms).unwrap();
        let snapshots = reopened.load_bot_snapshots().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].bot_id, "aapl");
    }

    #[test]
    fn multi_symbol_bot_fans_out_into_symbol_lanes() {
        let mut config = fixture_bundle();
        config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];

        let mut runtime = Runtime::from_config(&config);

        let summary = runtime.get_instance("aapl").unwrap();
        assert_eq!(summary.symbols, vec!["AAPL", "MSFT"]);
        assert_eq!(summary.open_symbol_count, 0);
        assert_eq!(summary.warmup_ready_symbols, 2);

        let lane_summaries = runtime.lane_summaries_for_bot("aapl").unwrap();
        assert_eq!(lane_summaries.len(), 2);
        assert_eq!(lane_summaries[0].symbol, "AAPL");
        assert_eq!(lane_summaries[1].symbol, "MSFT");

        let stream_symbols = runtime
            .effective_streams_for_dataplane()
            .into_iter()
            .map(|stream| stream.key.symbol)
            .collect::<Vec<_>>();
        assert_eq!(stream_symbols, vec!["AAPL", "MSFT"]);

        let started = runtime.start_instance("aapl").unwrap();
        assert_eq!(started.state, LaneRuntimeState::Running);
        let lane_summaries = runtime.lane_summaries_for_bot("aapl").unwrap();
        assert!(
            lane_summaries
                .iter()
                .all(|lane| matches!(lane.state, LaneRuntimeState::Running))
        );
    }
}
