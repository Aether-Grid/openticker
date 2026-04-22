use crate::connector_gateway::ConnectorGatewayRead;
use crate::repo::{RuntimeRepo, RuntimeRepoRead};
use crate::{ConnectorRegistry, InstanceSummary, LaneRuntimeState, Runtime, ServiceError};
use std::sync::{Arc, Mutex};

pub(crate) struct ReconciliationEngineRead<'a> {
    pub(crate) repo: RuntimeRepoRead<'a>,
    pub(crate) connectors: Arc<Mutex<ConnectorRegistry>>,
}

pub(crate) struct ReconciliationEngine<'a> {
    pub(crate) repo: RuntimeRepo<'a>,
    pub(crate) connectors: Arc<Mutex<ConnectorRegistry>>,
}

impl ReconciliationEngine<'_> {
    fn as_read(&self) -> ReconciliationEngineRead<'_> {
        ReconciliationEngineRead {
            repo: self.repo.as_read(),
            connectors: self.connectors.clone(),
        }
    }
}

impl ReconciliationEngineRead<'_> {
    pub(crate) fn connector_gateway(&self) -> ConnectorGatewayRead<'_> {
        ConnectorGatewayRead::from_parts(
            openticker_gateway::Gateway::new(self.connectors.clone()),
            self.repo,
        )
    }
}

impl Runtime {
    #[cfg(test)]
    pub(crate) fn reconciliation_engine(&self) -> ReconciliationEngineRead<'_> {
        ReconciliationEngineRead {
            repo: self.repo(),
            connectors: self.connectors.clone(),
        }
    }

    pub(crate) fn reconciliation_engine_mut(&mut self) -> ReconciliationEngine<'_> {
        let connectors = self.connectors.clone();
        ReconciliationEngine {
            repo: self.repo_mut(),
            connectors,
        }
    }

    pub(crate) fn run_startup_reconciliation(&mut self) -> Result<(), ServiceError> {
        self.reconciliation_engine_mut()
            .run_startup_reconciliation()
    }

    pub(crate) fn reconcile_instance_internal(
        &mut self,
        instance_id: &str,
        source: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        self.reconciliation_engine_mut()
            .reconcile_instance_internal(instance_id, source)
    }
}

impl ReconciliationEngine<'_> {
    pub(crate) fn run_startup_reconciliation(&mut self) -> Result<(), ServiceError> {
        let pending = self
            .repo
            .state
            .lanes
            .iter()
            .filter_map(|(instance_id, runtime)| {
                if matches!(runtime.state, LaneRuntimeState::Reconciling) {
                    Some(instance_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for instance_id in pending {
            if let Err(error) = self.reconcile_instance_internal(&instance_id, "startup") {
                self.mark_startup_reconciliation_failed(&instance_id, &error)?;
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn reconcile_instance_internal(
        &mut self,
        instance_id: &str,
        source: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        let account_id = self
            .repo
            .as_read()
            .instance(instance_id)?
            .config
            .account
            .clone();
        let needs_reconcile_transition = {
            let repo = self.repo.as_read();
            let instance = repo.instance(instance_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }
            !matches!(instance.state, LaneRuntimeState::Reconciling)
        };

        if needs_reconcile_transition {
            self.repo.reborrow().transition_instance(
                instance_id,
                LaneRuntimeState::Reconciling,
                "instance.reconcile_started",
            )?;
        }

        let mut assessment = self.as_read().assess_reconciliation(instance_id)?;
        let sync = self.sync_reconciliation_state(instance_id, source, &assessment)?;
        if sync.position_synced
            || sync.stale_local_open_orders_closed_count > 0
            || sync.remote_open_orders_backfilled_count > 0
        {
            assessment = self.as_read().assess_reconciliation(instance_id)?;
        }
        self.apply_reconciliation_assessment_state(instance_id, &assessment)?;
        self.refresh_account_ledger_for_assessment(&account_id, &assessment)?;
        self.finalize_reconciliation_result(instance_id, source, &assessment, &sync)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_temp_db_path, fixture_bundle_with_missing_account_instance};
    use openticker_storage::{BotSnapshotWrite, RuntimeJournal, SqliteRuntimeJournal};
    use std::sync::Arc;

    #[test]
    fn startup_reconciliation_failure_isolated_to_failing_instance() {
        let db_path = create_temp_db_path("reconcile-isolated-failure");
        let config = fixture_bundle_with_missing_account_instance(db_path.clone());

        let journal = Arc::new(
            SqliteRuntimeJournal::open(&db_path, config.global.storage.busy_timeout_ms)
                .expect("sqlite journal should open"),
        );
        for instance_id in ["aapl", "msft"] {
            journal
                .upsert_bot_snapshot(BotSnapshotWrite {
                    bot_id: instance_id.to_owned(),
                    state: "running".to_owned(),
                    execution_mode: "paper".to_owned(),
                    enabled: true,
                })
                .expect("snapshot should persist");
        }

        let mut runtime =
            Runtime::from_config_with_journal(&config, journal).expect("runtime should boot");

        let aapl = runtime.get_instance("aapl").expect("aapl should exist");
        assert_eq!(aapl.state, LaneRuntimeState::Running);
        assert!(!aapl.reconciliation_blocked);

        let msft = runtime.get_instance("msft").expect("msft should exist");
        assert_eq!(msft.state, LaneRuntimeState::Paused);
        assert!(msft.reconciliation_blocked);

        let aapl_started = runtime
            .start_instance("aapl")
            .expect("aapl should be startable");
        assert_eq!(aapl_started.state, LaneRuntimeState::Running);

        let msft_start = runtime.start_instance("msft");
        assert!(matches!(
            msft_start,
            Err(ServiceError::ReconciliationRequired { .. })
        ));

        let reconcile_events = runtime
            .recent_events_by_scope("reconcile", 20)
            .expect("events should load");
        assert!(reconcile_events.iter().any(|event| {
            event.kind == "reconcile.failed" && event.entity_id.as_deref() == Some("msft")
        }));
    }
}
