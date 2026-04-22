use crate::InstanceSummary;
use crate::{LaneRuntimeState, Runtime, ServiceError, execution_mode_is_live};

impl Runtime {
    /// Transitions an instance into the running state.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing, disabled, blocked, or not ready to run.
    pub fn start_instance(&mut self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        if self.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        let lane_ids = self.repo().lane_ids_for_bot_cloned(instance_id)?;
        let primary_lane_id = lane_ids
            .first()
            .cloned()
            .ok_or_else(|| ServiceError::InstanceNotFound(instance_id.to_owned()))?;

        let (enabled, account_id) = {
            let repo = self.repo();
            let instance = repo.instance(&primary_lane_id)?;
            (instance.config.enabled, instance.config.account.clone())
        };
        if !enabled {
            return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
        }
        {
            let repo = self.repo();
            let lanes = repo.lanes_for_bot(instance_id)?;
            if lanes.iter().any(|lane| lane.reconciliation_blocked) {
                return Err(ServiceError::ReconciliationRequired {
                    instance_id: instance_id.to_owned(),
                    reason: "startup reconciliation has unresolved differences".to_owned(),
                });
            }
            if lanes
                .iter()
                .any(|lane| matches!(lane.state, LaneRuntimeState::Reconciling))
            {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: LaneRuntimeState::Reconciling,
                    action: "start",
                });
            }
        }
        self.ensure_live_reconciliation_ready(instance_id, "start")?;
        self.connector_gateway()
            .ensure_account_connector_ready(&primary_lane_id, &account_id)?;
        self.refresh_account_ledger_from_connector(&account_id)?;
        for lane_id in &lane_ids {
            self.attempt_instance_warmup_backfill(lane_id, "start")?;
        }

        for lane_id in lane_ids {
            self.repo_mut().transition_instance(
                &lane_id,
                LaneRuntimeState::Running,
                "instance.started",
            )?;
        }

        self.get_instance(instance_id)
    }

    /// Transitions an instance into the stopped state.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing or state persistence fails.
    pub fn stop_instance(&mut self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        for lane_id in self.repo().lane_ids_for_bot_cloned(instance_id)? {
            self.repo_mut().transition_instance(
                &lane_id,
                LaneRuntimeState::Stopped,
                "instance.stopped",
            )?;
        }
        self.get_instance(instance_id)
    }

    /// Transitions an instance into the paused state.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing or state persistence fails.
    pub fn pause_instance(&mut self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        for lane_id in self.repo().lane_ids_for_bot_cloned(instance_id)? {
            self.repo_mut().transition_instance(
                &lane_id,
                LaneRuntimeState::Paused,
                "instance.paused",
            )?;
        }
        self.get_instance(instance_id)
    }

    /// Sets the global kill switch and pauses running instances when enabled.
    ///
    /// # Errors
    ///
    /// Returns an error when state transitions or persistence fail.
    pub fn set_kill_switch(&mut self, active: bool) -> Result<(), ServiceError> {
        if self.state.kill_switch_active == active {
            return Ok(());
        }

        self.state.kill_switch_active = active;
        self.append_service_event(
            if active {
                "risk.kill_switch_enabled"
            } else {
                "risk.kill_switch_cleared"
            },
            format!("active={active}"),
        )?;

        if active {
            let running_ids = self
                .state
                .lanes
                .iter()
                .filter_map(|(id, instance)| {
                    if matches!(instance.state, LaneRuntimeState::Running) {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            for instance_id in running_ids {
                self.transition_instance(
                    &instance_id,
                    LaneRuntimeState::Paused,
                    "instance.paused_by_kill_switch",
                )?;
            }
        }

        Ok(())
    }

    /// Resumes a paused instance into the running state.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is missing, disabled, blocked, or not ready to run.
    pub fn resume_instance(&mut self, instance_id: &str) -> Result<InstanceSummary, ServiceError> {
        if self.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        let lane_ids = self.repo().lane_ids_for_bot_cloned(instance_id)?;
        let primary_lane_id = lane_ids
            .first()
            .cloned()
            .ok_or_else(|| ServiceError::InstanceNotFound(instance_id.to_owned()))?;

        let (enabled, account_id) = {
            let repo = self.repo();
            let instance = repo.instance(&primary_lane_id)?;
            (instance.config.enabled, instance.config.account.clone())
        };
        if !enabled {
            return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
        }
        {
            let repo = self.repo();
            let lanes = repo.lanes_for_bot(instance_id)?;
            if lanes.iter().any(|lane| lane.reconciliation_blocked) {
                return Err(ServiceError::ReconciliationRequired {
                    instance_id: instance_id.to_owned(),
                    reason: "instance must reconcile successfully before resume".to_owned(),
                });
            }
            if lanes
                .iter()
                .any(|lane| matches!(lane.state, LaneRuntimeState::Reconciling))
            {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: LaneRuntimeState::Reconciling,
                    action: "resume",
                });
            }
        }
        self.ensure_live_reconciliation_ready(instance_id, "resume")?;
        self.connector_gateway()
            .ensure_account_connector_ready(&primary_lane_id, &account_id)?;
        self.refresh_account_ledger_from_connector(&account_id)?;
        for lane_id in &lane_ids {
            self.attempt_instance_warmup_backfill(lane_id, "resume")?;
        }

        for lane_id in lane_ids {
            self.repo_mut().transition_instance(
                &lane_id,
                LaneRuntimeState::Running,
                "instance.resumed",
            )?;
        }

        self.get_instance(instance_id)
    }

    /// Runs reconciliation for an instance and updates runtime safety state.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance cannot reconcile or reconciliation IO fails.
    pub fn reconcile_instance(
        &mut self,
        instance_id: &str,
    ) -> Result<InstanceSummary, ServiceError> {
        for lane_id in self.lane_ids_for_bot_cloned(instance_id)? {
            self.reconcile_instance_internal(&lane_id, "manual")?;
        }
        self.get_instance(instance_id)
    }

    fn ensure_live_reconciliation_ready(
        &self,
        instance_id: &str,
        action: &'static str,
    ) -> Result<(), ServiceError> {
        let repo = self.repo();
        let summary = repo.get_instance(instance_id)?;
        if !execution_mode_is_live(summary.execution_mode) {
            return Ok(());
        }

        let lanes = repo.lanes_for_bot(instance_id)?;
        for lane in lanes {
            let latest = repo.latest_reconciliation_for_lane(instance_id, &lane.lane_symbol)?;
            let latest = match latest {
                Some(latest) => Some(latest),
                None if summary.symbols.len() == 1 => {
                    repo.latest_reconciliation_for_bot(instance_id)?
                }
                None => None,
            };
            let Some(latest) = latest else {
                return Err(ServiceError::ReconciliationRequired {
                    instance_id: instance_id.to_owned(),
                    reason: format!(
                        "live mode requires successful manual reconciliation before {action} for {}",
                        lane.lane_symbol
                    ),
                });
            };

            if latest.source != "manual" {
                return Err(ServiceError::ReconciliationRequired {
                    instance_id: instance_id.to_owned(),
                    reason: format!(
                        "live mode requires manual reconciliation before {action} for {} (latest source={})",
                        lane.lane_symbol, latest.source
                    ),
                });
            }

            if !latest.safe_to_trade {
                return Err(ServiceError::ReconciliationRequired {
                    instance_id: instance_id.to_owned(),
                    reason: format!(
                        "live mode manual reconciliation before {action} for {} is unresolved: {}",
                        lane.lane_symbol, latest.reason
                    ),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_bundle, fixture_live_bundle};

    #[test]
    fn starts_and_pauses_instances() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);

        let started = runtime.start_instance("aapl").unwrap();
        assert_eq!(started.state, LaneRuntimeState::Running);

        let paused = runtime.pause_instance("aapl").unwrap();
        assert_eq!(paused.state, LaneRuntimeState::Paused);
    }

    #[test]
    fn kill_switch_prevents_resume() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);

        runtime.set_kill_switch(true).unwrap();
        let result = runtime.resume_instance("aapl");
        assert!(matches!(result, Err(ServiceError::KillSwitchEnabled)));
    }

    #[test]
    fn kill_switch_pauses_running_instances_and_records_events() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").unwrap();

        runtime.set_kill_switch(true).unwrap();

        let summary = runtime.get_instance("aapl").unwrap();
        assert_eq!(summary.state, LaneRuntimeState::Paused);
        assert!(runtime.kill_switch_active());

        let service_events = runtime.recent_events_by_scope("service", 10).unwrap();
        assert!(
            service_events
                .iter()
                .any(|event| event.kind == "risk.kill_switch_enabled")
        );

        let instance_events = runtime
            .recent_events_by_scope_and_entity("instance", "aapl", 10)
            .unwrap();
        assert!(
            instance_events
                .iter()
                .any(|event| event.kind == "instance.paused_by_kill_switch")
        );
    }

    #[test]
    fn live_mode_requires_manual_reconciliation_before_start_and_resume() {
        let config = fixture_live_bundle();
        let mut runtime = Runtime::from_config(&config);

        let start = runtime.start_instance("aapl");
        assert!(matches!(
            start,
            Err(ServiceError::ReconciliationRequired { reason, .. })
                if reason.contains("manual reconciliation before start")
        ));

        let resume = runtime.resume_instance("aapl");
        assert!(matches!(
            resume,
            Err(ServiceError::ReconciliationRequired { reason, .. })
                if reason.contains("manual reconciliation before resume")
        ));

        runtime.reconcile_instance("aapl").unwrap();

        let started = runtime.start_instance("aapl").unwrap();
        assert_eq!(started.state, LaneRuntimeState::Running);

        runtime.pause_instance("aapl").unwrap();
        let resumed = runtime.resume_instance("aapl").unwrap();
        assert_eq!(resumed.state, LaneRuntimeState::Running);
    }

    #[test]
    fn records_runtime_events_for_transitions() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);

        runtime.start_instance("aapl").unwrap();
        let events = runtime.recent_events(10).unwrap();
        assert!(events.iter().any(|event| event.kind == "instance.started"));
    }
}
