use crate::{
    AcceptedOrder, CyclePositionStep, CycleReconciliationContext, CycleReconciliationSnapshot,
    CycleRelatedEvent, CycleTrace, CycleTriggerKind, IndicatorSignal, OhlcvBar, ProcessBarOutcome,
    ProcessBarRisk, Runtime, ServiceError, SignalPhase, TraceIdentity, TradeIntent, unix_now_ms,
};
use openticker_lane::{LaneCycleContext, LaneCycleEngine, build_cycle_trace};
use openticker_trace::build_capital_state;
use std::time::Duration;

pub(super) struct RuntimeLaneCycleEngine<'a> {
    pub(super) runtime: &'a mut Runtime,
}

impl LaneCycleEngine for RuntimeLaneCycleEngine<'_> {
    type Error = ServiceError;
    type Evaluation = crate::ProcessBarEvaluation;
    type AcceptedOrder = AcceptedOrder;
    type PositionStep = CyclePositionStep;
    type CapitalState = crate::CycleCapitalState;
    type RelatedEvent = CycleRelatedEvent;
    type Risk = ProcessBarRisk;
    type Outcome = ProcessBarOutcome;

    fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error> {
        if self.runtime.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        Ok(())
    }

    fn lane_cycle_context(&self, instance_id: &str) -> Result<LaneCycleContext, Self::Error> {
        let instance = self.runtime.instance(instance_id)?;
        Ok(LaneCycleContext {
            bot_id: instance.config.id.clone(),
            account_id: instance.config.account.clone(),
            execution_connector: instance.config.execution_connector.clone(),
            symbol: instance.lane_symbol.clone(),
            has_position: instance.has_position,
        })
    }

    fn has_matching_cycle_trace(
        &self,
        bot_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        trigger_kind: CycleTriggerKind,
        signal: Option<IndicatorSignal>,
    ) -> Result<bool, Self::Error> {
        self.runtime
            .has_matching_cycle_trace(bot_id, symbol, bar, phase, trigger_kind, signal)
    }

    fn duplicate_cycle_replay_outcome(
        &self,
        bot_id: String,
        symbol: String,
        phase: SignalPhase,
        has_position: bool,
    ) -> Self::Outcome {
        Runtime::duplicate_cycle_replay_outcome(bot_id, symbol, phase, has_position)
    }

    fn sync_account_ledger(&mut self, account_id: &str) -> Result<(), Self::Error> {
        self.runtime
            .repo()
            .sync_account_ledger_from_instances(account_id)
    }

    fn capture_cycle_capital_state(
        &self,
        account_id: &str,
        bot_id: &str,
        symbol: &str,
    ) -> Self::CapitalState {
        self.runtime
            .capture_cycle_capital_state(account_id, bot_id, symbol)
    }

    fn validate_account_connector_kind(
        &self,
        instance_id: &str,
        account_id: &str,
        execution_connector: &str,
    ) -> Result<(), Self::Error> {
        let account_kind = self
            .runtime
            .catalog
            .accounts
            .get(account_id)
            .ok_or_else(|| {
                ServiceError::InvalidConfiguration(format!(
                    "instance `{instance_id}` references unknown account `{account_id}`"
                ))
            })?
            .kind
            .clone();
        if account_kind != execution_connector {
            return Err(ServiceError::InvalidConfiguration(format!(
                "instance `{instance_id}` execution_connector `{execution_connector}` does not match account `{account_id}` kind `{account_kind}`"
            )));
        }

        Ok(())
    }

    fn ensure_account_connector_ready(
        &self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<(), Self::Error> {
        self.runtime
            .connector_gateway()
            .ensure_account_connector_ready(instance_id, account_id)
    }

    fn ensure_connector_execution_constraints(
        &mut self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<(), Self::Error> {
        self.runtime
            .ensure_lane_connector_execution_constraints(instance_id, account_id)
    }

    fn refresh_daily_loss_rollover(&mut self, date: chrono::NaiveDate) {
        self.runtime.refresh_daily_loss_rollover(date);
    }

    fn process_pending_warmup_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<Option<Self::Outcome>, Self::Error> {
        self.runtime
            .process_pending_warmup_bar(instance_id, bar, phase)
    }

    fn process_pending_recovery_bar(
        &mut self,
        instance_id: &str,
        phase: SignalPhase,
    ) -> Result<Option<Self::Outcome>, Self::Error> {
        self.runtime
            .process_pending_recovery_bar(instance_id, phase)
    }

    fn evaluate_process_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<Self::Evaluation, Self::Error> {
        let account_id = self.runtime.instance(instance_id)?.config.account.clone();
        let account_risk = self.runtime.repo().account_risk_snapshot(&account_id);
        self.runtime.processing_planner_mut().evaluate_process_bar(
            instance_id,
            bar,
            phase,
            account_risk,
        )
    }

    fn evaluate_manual_signal(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        signal: IndicatorSignal,
    ) -> Result<Self::Evaluation, Self::Error> {
        let account_id = self.runtime.instance(instance_id)?.config.account.clone();
        let account_risk = self.runtime.repo().account_risk_snapshot(&account_id);
        self.runtime
            .processing_planner_mut()
            .evaluate_manual_signal(instance_id, bar, phase, signal, account_risk)
    }

    fn append_process_bar_records(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        evaluation: &Self::Evaluation,
        trace_id: &str,
    ) -> Result<Vec<Self::RelatedEvent>, Self::Error> {
        self.runtime
            .processing_executor_mut()
            .append_process_bar_records(instance_id, bar, phase, evaluation, trace_id)
    }

    fn apply_risk_decision_effects(
        &mut self,
        instance_id: &str,
        account_id: &str,
        connector_kind: &str,
        symbol: &str,
        bar: &OhlcvBar,
        evaluation: &Self::Evaluation,
        trace_id: &str,
    ) -> Result<
        (
            Self::Risk,
            Option<Self::AcceptedOrder>,
            Vec<Self::RelatedEvent>,
        ),
        Self::Error,
    > {
        self.runtime
            .processing_executor_mut()
            .apply_risk_decision_effects(
                instance_id,
                account_id,
                connector_kind,
                symbol,
                bar,
                evaluation.intent,
                evaluation.next_has_position,
                evaluation.order_quantity,
                evaluation.order_ledger_outcome,
                &evaluation.risk_decision,
                trace_id,
            )
    }

    fn apply_process_bar_state(
        &mut self,
        instance_id: &str,
        account_id: &str,
        bar: &OhlcvBar,
        evaluation: &Self::Evaluation,
        accepted_order: Option<&Self::AcceptedOrder>,
        trace_id: &str,
    ) -> Result<Self::PositionStep, Self::Error> {
        self.runtime
            .processing_executor_mut()
            .apply_process_bar_state(
                instance_id,
                account_id,
                bar,
                evaluation.has_position_before,
                evaluation.next_has_position,
                accepted_order,
                evaluation.order_ledger_outcome,
                &evaluation.risk_decision,
                trace_id,
            )
    }

    fn persist_cycle_trace_for_evaluation(
        &self,
        trace: &TraceIdentity,
        account_id: &str,
        bar: &OhlcvBar,
        evaluation: &Self::Evaluation,
        accepted_order: Option<&Self::AcceptedOrder>,
        position_step: Self::PositionStep,
        capital_before: Self::CapitalState,
        related_events: Vec<Self::RelatedEvent>,
    ) -> Result<(), Self::Error> {
        self.runtime.persist_cycle_trace_for_evaluation(
            trace,
            account_id,
            bar,
            evaluation,
            accepted_order,
            position_step,
            capital_before,
            related_events,
        )
    }

    fn update_last_dispatched_bar_timestamp(
        &mut self,
        instance_id: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Self::Error> {
        let instance = self.runtime.instance_mut(instance_id)?;
        instance.last_dispatched_bar_timestamp = Some(timestamp);
        Ok(())
    }

    fn outcome_from_evaluation(
        &self,
        instance_id: &str,
        context: &LaneCycleContext,
        phase: SignalPhase,
        evaluation: &Self::Evaluation,
        risk: Self::Risk,
    ) -> Result<Self::Outcome, Self::Error> {
        let has_position = self.runtime.instance(instance_id)?.has_position;
        Ok(ProcessBarOutcome {
            instance_id: context.bot_id.clone(),
            bot_id: context.bot_id.clone(),
            symbol: context.symbol.clone(),
            phase,
            signal: evaluation.signal,
            signal_metadata: (!evaluation.signal_metadata.is_empty())
                .then_some(evaluation.signal_metadata.clone()),
            intent: evaluation.intent,
            strategy_rationale: evaluation.strategy_rationale.clone(),
            has_position,
            risk,
        })
    }

    fn record_successful_cycle_latency(&mut self, elapsed: Duration) {
        self.runtime
            .state
            .observability
            .process_bar_latency
            .record_elapsed(elapsed);
    }
}

impl Runtime {
    pub(super) fn capture_cycle_capital_state(
        &self,
        account_id: &str,
        bot_id: &str,
        symbol: &str,
    ) -> crate::CycleCapitalState {
        let snapshot = self.ledger_snapshot();
        build_capital_state(
            snapshot
                .accounts
                .iter()
                .find(|entry| entry.id == account_id),
            snapshot
                .bots
                .iter()
                .find(|entry| entry.id == bot_id && entry.account_id == account_id),
            snapshot.lanes.iter().find(|entry| {
                entry.owner.account_id == account_id
                    && entry.owner.bot_id == bot_id
                    && entry.owner.symbol == symbol
            }),
        )
    }

    pub(super) fn duplicate_cycle_replay_outcome(
        bot_id: String,
        symbol: String,
        phase: SignalPhase,
        has_position: bool,
    ) -> ProcessBarOutcome {
        ProcessBarOutcome {
            instance_id: bot_id.clone(),
            bot_id,
            symbol,
            phase,
            signal: IndicatorSignal::None,
            signal_metadata: None,
            intent: TradeIntent::NoOp,
            strategy_rationale: Some("duplicate_cycle_replay_skipped".to_owned()),
            has_position,
            risk: ProcessBarRisk::Allowed,
        }
    }

    pub(super) fn has_matching_cycle_trace(
        &self,
        bot_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        trigger_kind: CycleTriggerKind,
        signal: Option<IndicatorSignal>,
    ) -> Result<bool, ServiceError> {
        let phase_label = match phase {
            SignalPhase::Preview => "preview",
            SignalPhase::Confirmed => "confirmed",
        };
        let bar_timestamp = bar.timestamp.to_rfc3339();
        let traces = self.repo().recent_cycle_traces_for_bot(
            bot_id,
            Some(symbol),
            Some(phase_label),
            None,
            Some(&bar_timestamp),
            50,
        )?;

        Ok(traces
            .into_iter()
            .filter_map(|record| serde_json::from_str::<CycleTrace>(&record.payload_json).ok())
            .any(|trace| {
                trace.trigger.trigger_kind == trigger_kind
                    && trace.trigger.phase == phase
                    && trace.trigger.bar_timestamp == bar_timestamp
                    && (trace.trigger.close - bar.close).abs() < f64::EPSILON
                    && signal.is_none_or(|expected| trace.signal_step.signal == expected)
            }))
    }

    pub(super) fn capture_cycle_reconciliation_context(
        &self,
        bot_id: &str,
        symbol: &str,
    ) -> Result<CycleReconciliationContext, ServiceError> {
        let latest = self
            .repo()
            .latest_reconciliation_for_lane(bot_id, symbol)?
            .map(|record| CycleReconciliationSnapshot {
                source: record.source,
                safe_to_trade: record.safe_to_trade,
                local_open_orders: record.local_open_orders,
                connector_open_orders: record.connector_open_orders,
                local_has_position: record.local_has_position,
                connector_has_position: record.connector_has_position,
                reason: record.reason,
                created_at_ms: record.created_at_ms,
            });
        Ok(CycleReconciliationContext { latest })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(super) fn persist_cycle_trace_for_evaluation(
        &self,
        trace: &TraceIdentity,
        account_id: &str,
        bar: &OhlcvBar,
        evaluation: &crate::ProcessBarEvaluation,
        accepted_order: Option<&AcceptedOrder>,
        position_step: CyclePositionStep,
        capital_before: crate::CycleCapitalState,
        related_events: Vec<CycleRelatedEvent>,
    ) -> Result<(), ServiceError> {
        let capital_after =
            self.capture_cycle_capital_state(account_id, &trace.bot_id, &trace.symbol);
        let reconciliation_context =
            self.capture_cycle_reconciliation_context(&trace.bot_id, &trace.symbol)?;
        let cycle_trace = build_cycle_trace(
            trace,
            bar,
            evaluation,
            accepted_order,
            position_step,
            capital_before,
            capital_after,
            reconciliation_context,
            related_events,
            unix_now_ms(),
        );

        self.repo().append_cycle_trace(&cycle_trace)
    }
}
