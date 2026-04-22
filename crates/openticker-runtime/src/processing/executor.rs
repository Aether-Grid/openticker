use super::executor_engine::ProcessingExecutor;
use crate::{
    AcceptedOrder, CyclePositionStep, CycleRelatedEvent, OhlcvBar, OrderLedgerOutcome,
    ProcessBarEvaluation, ProcessBarRisk, RiskDecision, ServiceError, SignalPhase, TradeIntent,
};
use openticker_lane::{
    append_process_bar_records as append_lane_process_bar_records, apply_process_bar_state_effects,
    apply_risk_decision_effects as apply_lane_risk_decision_effects,
};

impl ProcessingExecutor<'_> {
    pub(crate) fn append_process_bar_records(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        evaluation: &ProcessBarEvaluation,
        trace_id: &str,
    ) -> Result<Vec<CycleRelatedEvent>, ServiceError> {
        append_lane_process_bar_records(self, instance_id, bar, phase, evaluation, trace_id)
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn apply_risk_decision_effects(
        &mut self,
        instance_id: &str,
        account_id: &str,
        connector_kind: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        next_has_position: bool,
        order_quantity: f64,
        order_ledger_outcome: Option<OrderLedgerOutcome>,
        risk_decision: &RiskDecision,
        trace_id: &str,
    ) -> Result<
        (
            ProcessBarRisk,
            Option<AcceptedOrder>,
            Vec<CycleRelatedEvent>,
        ),
        ServiceError,
    > {
        apply_lane_risk_decision_effects(
            self,
            instance_id,
            account_id,
            connector_kind,
            symbol,
            bar,
            intent,
            next_has_position,
            order_quantity,
            order_ledger_outcome,
            risk_decision,
            trace_id,
        )
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn apply_process_bar_state(
        &mut self,
        instance_id: &str,
        account_id: &str,
        bar: &OhlcvBar,
        has_position_before: bool,
        next_has_position: bool,
        accepted_order: Option<&AcceptedOrder>,
        order_ledger_outcome: Option<OrderLedgerOutcome>,
        risk_decision: &RiskDecision,
        trace_id: &str,
    ) -> Result<CyclePositionStep, ServiceError> {
        apply_process_bar_state_effects(
            self,
            instance_id,
            account_id,
            bar,
            has_position_before,
            next_has_position,
            order_ledger_outcome,
            risk_decision,
            accepted_order,
            trace_id,
        )
    }
}
