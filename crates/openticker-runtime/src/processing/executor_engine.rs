use crate::connector_gateway::ConnectorGatewayRead;
use crate::portfolio_adapter::PortfolioAdapter;
use crate::repo::RuntimeRepo;
use crate::{
    AcceptedOrder, ConnectorRegistry, ExecutionRequest, OhlcvBar, ProcessBarEvaluation,
    ProcessBarRisk, RiskDecision, Runtime, ServiceError, SignalPhase, TradeIntent,
    apply_process_bar_fill_state, effective_position_quantity, inventory_transition_error,
};
use openticker_lane::{LaneExecutionEngine, PositionRecordState, ProcessBarStateMutation};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct ProcessingExecutor<'a> {
    repo: RuntimeRepo<'a>,
    connectors: Arc<Mutex<ConnectorRegistry>>,
}

impl Runtime {
    pub(crate) fn processing_executor_mut(&mut self) -> ProcessingExecutor<'_> {
        let connectors = self.connectors.clone();
        ProcessingExecutor {
            repo: self.repo_mut(),
            connectors,
        }
    }
}

impl LaneExecutionEngine for ProcessingExecutor<'_> {
    type Error = ServiceError;
    type Risk = ProcessBarRisk;

    fn append_signal_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        signal: crate::IndicatorSignal,
        signal_metadata: &crate::SignalMetadata,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_signal_record_with_trace(
            instance_id,
            bar,
            phase,
            signal,
            signal_metadata,
            Some(trace_id),
        )
    }

    fn append_intent_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        evaluation: &ProcessBarEvaluation,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_intent_record_with_trace(
            instance_id,
            bar,
            evaluation.signal,
            &evaluation.signal_metadata,
            evaluation.intent,
            evaluation.strategy_rationale.as_deref(),
            evaluation.has_position_before,
            Some(trace_id),
        )
    }

    fn append_risk_decision_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        risk_decision: &RiskDecision,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_risk_decision_record_with_trace(
            instance_id,
            bar,
            intent,
            risk_decision,
            Some(trace_id),
        )
    }

    fn append_runtime_event(
        &self,
        scope: &str,
        instance_id: &str,
        kind: &str,
        payload: String,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_runtime_event_with_trace(
            scope,
            Some(instance_id),
            kind,
            payload,
            Some(trace_id),
        )
    }

    fn append_order_record(
        &self,
        instance_id: &str,
        client_order_id: &str,
        intent: TradeIntent,
        status: &str,
        price: f64,
        quantity: f64,
        bar: &OhlcvBar,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_order_record_with_trace(
            instance_id,
            client_order_id,
            intent,
            status,
            price,
            quantity,
            Some(bar),
            Some(trace_id),
        )
    }

    fn append_fill_record(
        &self,
        instance_id: &str,
        order: &AcceptedOrder,
        bar: &OhlcvBar,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_fill_record_with_trace(
            instance_id,
            &order.client_order_id,
            order.price,
            order.quantity,
            order.fee_asset.clone(),
            order.fee_amount,
            order.fee_normalized_usd,
            Some(bar),
            Some(trace_id),
        )
    }

    fn append_position_record(
        &self,
        instance_id: &str,
        position_record: PositionRecordState,
        bar: &OhlcvBar,
        trace_id: &str,
    ) -> Result<(), Self::Error> {
        self.repo.as_read().append_position_record_with_trace(
            instance_id,
            position_record.has_position,
            position_record.quantity,
            position_record.entry_price,
            position_record.realized_pnl_usd,
            "order_filled",
            Some(bar),
            Some(trace_id),
        )
    }

    fn record_ledger_rejection_with_trace(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: crate::OrderLedgerOutcome,
        trace_id: &str,
    ) -> Result<serde_json::Value, Self::Error> {
        self.portfolio_adapter_mut()
            .record_ledger_rejection_with_trace(
                instance_id,
                account_id,
                symbol,
                bar,
                intent,
                ledger_outcome,
                Some(trace_id),
            )
    }

    fn record_ledger_rejection(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: crate::OrderLedgerOutcome,
    ) -> Result<(), Self::Error> {
        self.portfolio_adapter_mut().record_ledger_rejection(
            instance_id,
            account_id,
            symbol,
            bar,
            intent,
            ledger_outcome,
        )
    }

    fn increment_ledger_reserve_attempts(&mut self) {
        self.repo.state.observability.ledger_reserve_attempts_total = self
            .repo
            .state
            .observability
            .ledger_reserve_attempts_total
            .saturating_add(1);
    }

    fn increment_risk_rejects_total(&mut self) {
        self.repo.state.observability.risk_rejects_total = self
            .repo
            .state
            .observability
            .risk_rejects_total
            .saturating_add(1);
    }

    fn bot_budget_pct(&self, instance_id: &str) -> Result<f64, Self::Error> {
        Ok(self.repo.as_read().instance(instance_id)?.config.budget.pct)
    }

    fn ledger_owner_path_for_instance(
        &self,
        instance_id: &str,
    ) -> Result<crate::LedgerOwnerPath, Self::Error> {
        self.repo
            .as_read()
            .ledger_owner_path_for_instance(instance_id)
    }

    fn try_reserve_open(
        &mut self,
        account_id: &str,
        owner: &crate::LedgerOwnerPath,
        reserve_notional_usd: f64,
        bot_budget_pct: f64,
    ) -> Result<Result<(), crate::ReservationError>, Self::Error> {
        let ledger = self.repo.as_read().account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        Ok(ledger.try_reserve_open(owner, reserve_notional_usd, bot_budget_pct))
    }

    fn release_reservation(
        &mut self,
        account_id: &str,
        owner: &crate::LedgerOwnerPath,
        reserve_notional_usd: f64,
    ) -> Result<(), Self::Error> {
        let ledger = self.repo.as_read().account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        ledger.release_reservation(owner, reserve_notional_usd);
        Ok(())
    }

    fn reconcile_open_fill(
        &mut self,
        account_id: &str,
        owner: &crate::LedgerOwnerPath,
        filled_notional_usd: f64,
        reserve_notional_usd: f64,
    ) -> Result<(), Self::Error> {
        let ledger = self.repo.as_read().account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        ledger.reconcile_open_fill(owner, filled_notional_usd, reserve_notional_usd);
        Ok(())
    }

    fn submit_order(
        &mut self,
        instance_id: &str,
        account_id: &str,
        connector_kind: &str,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, Self::Error> {
        self.connector_gateway()
            .submit_order(instance_id, account_id, connector_kind, request)
    }

    fn record_execution_submit_latency(&mut self, elapsed: Duration) {
        self.repo
            .state
            .observability
            .execution_submit_latency
            .record_elapsed(elapsed);
    }

    fn apply_fill_state_mutation(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        next_has_position: bool,
        accepted_order: Option<&AcceptedOrder>,
        order_ledger_outcome: Option<crate::OrderLedgerOutcome>,
        risk_decision: &RiskDecision,
    ) -> Result<ProcessBarStateMutation, Self::Error> {
        let instance = self.repo.instance_mut(instance_id)?;
        apply_process_bar_fill_state(
            instance,
            bar,
            next_has_position,
            accepted_order,
            order_ledger_outcome,
            risk_decision,
        )
        .map_err(|failure| inventory_transition_error(instance_id, failure.action, failure.error))
    }

    fn release_position(
        &mut self,
        account_id: &str,
        owner: &crate::LedgerOwnerPath,
        released_notional_usd: f64,
    ) -> Result<(), Self::Error> {
        let ledger = self.repo.as_read().account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        ledger.release_position(owner, released_notional_usd);
        Ok(())
    }

    fn sync_account_ledger_from_instances(&self, account_id: &str) -> Result<(), Self::Error> {
        self.repo
            .as_read()
            .sync_account_ledger_from_instances(account_id)
    }

    fn position_record_state(&self, instance_id: &str) -> Result<PositionRecordState, Self::Error> {
        let repo = self.repo.as_read();
        let instance = repo.instance(instance_id)?;
        Ok(PositionRecordState {
            has_position: instance.has_position,
            quantity: effective_position_quantity(instance),
            entry_price: instance.entry_price,
            realized_pnl_usd: instance.realized_pnl_usd,
        })
    }

    fn allowed_risk(&self) -> Self::Risk {
        ProcessBarRisk::Allowed
    }

    fn rejected_risk(&self, reason: String) -> Self::Risk {
        ProcessBarRisk::Rejected { reason }
    }
}

impl ProcessingExecutor<'_> {
    fn portfolio_adapter_mut(&mut self) -> PortfolioAdapter<'_> {
        PortfolioAdapter::from_parts(self.repo.reborrow())
    }

    fn connector_gateway(&self) -> ConnectorGatewayRead<'_> {
        ConnectorGatewayRead::from_parts(
            openticker_gateway::Gateway::new(self.connectors.clone()),
            self.repo.as_read(),
        )
    }
}
