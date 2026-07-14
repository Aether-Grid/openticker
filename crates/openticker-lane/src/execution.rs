use crate::position::{PositionRecordState, ProcessBarStateMutation};
use crate::signals::ProcessBarEvaluation;
use crate::trace::{
    indicator_signal_label, ledger_outcome_reason_code, signal_phase_label,
    strategy_signal_source_label, trace_event, trade_intent_label,
};
use openticker_core::{IndicatorSignal, OhlcvBar, SignalMetadata, SignalPhase, TradeIntent};
use openticker_execution::{AcceptedOrder, ExecutionRequest, OrderLedgerOutcome};
use openticker_ledger::{LedgerOwnerPath, ReservationError, calculate_position_notional_usd};
use openticker_risk::RiskDecision;
use openticker_trace::{PositionStep, RelatedEvent};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[allow(clippy::missing_errors_doc, clippy::too_many_arguments)]
pub trait LaneExecutionEngine {
    type Error;
    type Risk;

    fn append_signal_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        signal: IndicatorSignal,
        signal_metadata: &SignalMetadata,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
    fn append_intent_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        evaluation: &ProcessBarEvaluation,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
    fn append_risk_decision_record(
        &self,
        instance_id: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        risk_decision: &RiskDecision,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
    fn append_runtime_event(
        &self,
        scope: &str,
        instance_id: &str,
        kind: &str,
        payload: String,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
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
    ) -> Result<(), Self::Error>;
    fn append_fill_record(
        &self,
        instance_id: &str,
        order: &AcceptedOrder,
        bar: &OhlcvBar,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
    fn append_position_record(
        &self,
        instance_id: &str,
        position_record: PositionRecordState,
        bar: &OhlcvBar,
        trace_id: &str,
    ) -> Result<(), Self::Error>;
    fn record_ledger_rejection_with_trace(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: OrderLedgerOutcome,
        trace_id: &str,
    ) -> Result<Value, Self::Error>;
    fn record_ledger_rejection(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: OrderLedgerOutcome,
    ) -> Result<(), Self::Error>;
    fn increment_ledger_reserve_attempts(&mut self);
    fn increment_risk_rejects_total(&mut self);
    fn bot_budget_pct(&self, instance_id: &str) -> Result<f64, Self::Error>;
    fn ledger_owner_path_for_instance(
        &self,
        instance_id: &str,
    ) -> Result<LedgerOwnerPath, Self::Error>;
    fn try_reserve_open(
        &mut self,
        account_id: &str,
        owner: &LedgerOwnerPath,
        reserve_notional_usd: f64,
        bot_budget_pct: f64,
    ) -> Result<Result<(), ReservationError>, Self::Error>;
    fn release_reservation(
        &mut self,
        account_id: &str,
        owner: &LedgerOwnerPath,
        reserve_notional_usd: f64,
    ) -> Result<(), Self::Error>;
    fn reconcile_open_fill(
        &mut self,
        account_id: &str,
        owner: &LedgerOwnerPath,
        filled_notional_usd: f64,
        reserve_notional_usd: f64,
    ) -> Result<(), Self::Error>;
    fn submit_order(
        &mut self,
        instance_id: &str,
        account_id: &str,
        connector_kind: &str,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, Self::Error>;
    fn record_execution_submit_latency(&mut self, elapsed: Duration);
    fn apply_fill_state_mutation(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        next_has_position: bool,
        accepted_order: Option<&AcceptedOrder>,
        order_ledger_outcome: Option<OrderLedgerOutcome>,
        risk_decision: &RiskDecision,
    ) -> Result<ProcessBarStateMutation, Self::Error>;
    fn release_position(
        &mut self,
        account_id: &str,
        owner: &LedgerOwnerPath,
        released_notional_usd: f64,
    ) -> Result<(), Self::Error>;
    fn sync_account_ledger_from_instances(&self, account_id: &str) -> Result<(), Self::Error>;
    fn position_record_state(&self, instance_id: &str) -> Result<PositionRecordState, Self::Error>;
    fn allowed_risk(&self) -> Self::Risk;
    fn rejected_risk(&self, reason: String) -> Self::Risk;
}

/// Appends journal and runtime-event records for a completed lane evaluation.
///
/// # Errors
///
/// Propagates engine errors from persistence and event emission.
pub fn append_process_bar_records<E: LaneExecutionEngine>(
    engine: &E,
    instance_id: &str,
    bar: &OhlcvBar,
    phase: SignalPhase,
    evaluation: &ProcessBarEvaluation,
    trace_id: &str,
) -> Result<Vec<RelatedEvent>, E::Error> {
    let mut related_events = Vec::new();
    if evaluation.signal != IndicatorSignal::None {
        engine.append_signal_record(
            instance_id,
            bar,
            phase,
            evaluation.signal,
            &evaluation.signal_metadata,
            trace_id,
        )?;
    }

    engine.append_intent_record(instance_id, bar, evaluation, trace_id)?;
    let intent_payload = format!(
        "signal={},signal_source={},intent={},has_position_before={}",
        indicator_signal_label(evaluation.signal),
        strategy_signal_source_label(evaluation.signal_source),
        trade_intent_label(evaluation.intent),
        evaluation.has_position_before,
    );
    engine.append_runtime_event(
        "intent",
        instance_id,
        "intent.generated",
        intent_payload.clone(),
        trace_id,
    )?;
    related_events.push(trace_event(
        "intent",
        "intent.generated",
        Value::String(intent_payload),
    ));
    engine.append_risk_decision_record(
        instance_id,
        bar,
        evaluation.intent,
        &evaluation.risk_decision,
        trace_id,
    )?;

    if let Some(reason) = evaluation.order_quantity_adjustment_reason.as_deref() {
        engine.append_runtime_event(
            "order",
            instance_id,
            "order.quantity_adjusted",
            reason.to_string(),
            trace_id,
        )?;
        related_events.push(trace_event(
            "order",
            "order.quantity_adjusted",
            Value::String(reason.to_owned()),
        ));
    }

    if evaluation.signal != IndicatorSignal::None {
        let signal_payload = format!(
            "phase={},signal={},signal_source={},close={}",
            signal_phase_label(phase),
            indicator_signal_label(evaluation.signal),
            strategy_signal_source_label(evaluation.signal_source),
            bar.close,
        );
        engine.append_runtime_event(
            "signal",
            instance_id,
            "signal.emitted",
            signal_payload.clone(),
            trace_id,
        )?;
        related_events.push(trace_event(
            "signal",
            "signal.emitted",
            Value::String(signal_payload),
        ));
    }

    Ok(related_events)
}

/// Applies the risk/execution side effects for a completed lane evaluation.
///
/// # Errors
///
/// Propagates engine errors from ledger operations, connector submission, or
/// journal/event persistence.
///
/// # Panics
///
/// Panics if an exhausted ledger outcome reaches the guarded branch without
/// matching the preceding check, which would indicate an internal inconsistency.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::type_complexity
)]
pub fn apply_risk_decision_effects<E: LaneExecutionEngine>(
    engine: &mut E,
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
) -> Result<(E::Risk, Option<AcceptedOrder>, Vec<RelatedEvent>), E::Error> {
    let mut related_events = Vec::new();
    if matches!(
        order_ledger_outcome,
        Some(OrderLedgerOutcome::BotExhausted | OrderLedgerOutcome::AccountExhausted)
    ) {
        let ledger_outcome = order_ledger_outcome.expect("checked above");
        let rejection_payload = engine.record_ledger_rejection_with_trace(
            instance_id,
            account_id,
            symbol,
            bar,
            intent,
            ledger_outcome,
            trace_id,
        )?;
        related_events.push(trace_event("risk", "risk.rejected", rejection_payload));
        return Ok((
            engine.rejected_risk(ledger_outcome_reason_code(ledger_outcome).to_owned()),
            None,
            related_events,
        ));
    }

    match risk_decision {
        RiskDecision::Allow(allowed_intent) => {
            let allowed_intent = *allowed_intent;
            let risk_allowed_payload = json!({
                "symbol": symbol,
                "bar_timestamp": bar.timestamp.to_rfc3339(),
                "intent": trade_intent_label(allowed_intent),
                "decision": "allowed",
                "reason": serde_json::Value::Null,
            });
            engine.append_runtime_event(
                "risk",
                instance_id,
                "risk.allowed",
                risk_allowed_payload.to_string(),
                trace_id,
            )?;
            related_events.push(trace_event("risk", "risk.allowed", risk_allowed_payload));

            if matches!(order_ledger_outcome, Some(OrderLedgerOutcome::Dust)) {
                let dust_payload = json!({
                    "intent": trade_intent_label(intent),
                    "symbol": symbol,
                    "bar_timestamp": bar.timestamp.to_rfc3339(),
                    "price": bar.close,
                    "reason": "remaining ledger room fell below exchange minimum",
                });
                engine.append_runtime_event(
                    "order",
                    instance_id,
                    "order.ledger_dust",
                    dust_payload.to_string(),
                    trace_id,
                )?;
                related_events.push(trace_event("order", "order.ledger_dust", dust_payload));
                return Ok((engine.allowed_risk(), None, related_events));
            }

            if allowed_intent != TradeIntent::NoOp {
                let reserve_notional_usd =
                    if matches!(allowed_intent, TradeIntent::OpenLong | TradeIntent::AddLong) {
                        Some(calculate_position_notional_usd(order_quantity, bar.close))
                    } else {
                        None
                    };
                if matches!(allowed_intent, TradeIntent::OpenLong | TradeIntent::AddLong) {
                    engine.increment_ledger_reserve_attempts();
                    let bot_budget_pct = engine.bot_budget_pct(instance_id)?;
                    let owner = engine.ledger_owner_path_for_instance(instance_id)?;
                    if let Err(error) = engine.try_reserve_open(
                        account_id,
                        &owner,
                        reserve_notional_usd.unwrap_or(0.0),
                        bot_budget_pct,
                    )? {
                        engine.record_ledger_rejection(
                            instance_id,
                            account_id,
                            symbol,
                            bar,
                            intent,
                            match error {
                                ReservationError::BotCapacityExceeded => {
                                    OrderLedgerOutcome::BotExhausted
                                }
                                ReservationError::AccountCapacityExceeded => {
                                    OrderLedgerOutcome::AccountExhausted
                                }
                            },
                        )?;
                        return Ok((
                            engine.rejected_risk(match error {
                                ReservationError::BotCapacityExceeded => {
                                    "bot_ledger_exhausted".to_owned()
                                }
                                ReservationError::AccountCapacityExceeded => {
                                    "account_ledger_exhausted".to_owned()
                                }
                            }),
                            None,
                            related_events,
                        ));
                    }
                }

                let execution_request = ExecutionRequest {
                    instance_id: instance_id.to_owned(),
                    symbol: symbol.to_owned(),
                    timestamp: bar.timestamp,
                    intent: allowed_intent,
                    price: bar.close,
                    quantity: order_quantity,
                };
                let execution_submit_started_at = Instant::now();
                let accepted_order = match engine.submit_order(
                    instance_id,
                    account_id,
                    connector_kind,
                    &execution_request,
                ) {
                    Ok(accepted_order) => accepted_order,
                    Err(error) => {
                        if let Some(reserve_notional_usd) = reserve_notional_usd {
                            let owner = engine.ledger_owner_path_for_instance(instance_id)?;
                            engine.release_reservation(account_id, &owner, reserve_notional_usd)?;
                        }
                        return Err(error);
                    }
                };
                engine.record_execution_submit_latency(execution_submit_started_at.elapsed());

                if let Some(reserve_notional_usd) = reserve_notional_usd {
                    let filled_notional_usd = calculate_position_notional_usd(
                        accepted_order.quantity,
                        accepted_order.price,
                    );
                    let owner = engine.ledger_owner_path_for_instance(instance_id)?;
                    engine.reconcile_open_fill(
                        account_id,
                        &owner,
                        filled_notional_usd,
                        reserve_notional_usd,
                    )?;
                }

                engine.append_order_record(
                    instance_id,
                    &accepted_order.client_order_id,
                    allowed_intent,
                    "submitted",
                    accepted_order.price,
                    accepted_order.quantity,
                    bar,
                    trace_id,
                )?;
                engine.append_fill_record(instance_id, &accepted_order, bar, trace_id)?;

                let order_submitted_payload = json!({
                    "account_id": account_id,
                    "connector_kind": connector_kind,
                    "client_order_id": accepted_order.client_order_id.as_str(),
                    "symbol": symbol,
                    "bar_timestamp": bar.timestamp.to_rfc3339(),
                    "intent": trade_intent_label(allowed_intent),
                    "price": accepted_order.price,
                    "quantity": accepted_order.quantity,
                    "bar_close": bar.close,
                });
                engine.append_runtime_event(
                    "order",
                    instance_id,
                    "order.submitted",
                    order_submitted_payload.to_string(),
                    trace_id,
                )?;
                related_events.push(trace_event(
                    "order",
                    "order.submitted",
                    order_submitted_payload,
                ));
                let order_filled_payload = json!({
                    "account_id": account_id,
                    "connector_kind": connector_kind,
                    "client_order_id": accepted_order.client_order_id.as_str(),
                    "symbol": symbol,
                    "bar_timestamp": bar.timestamp.to_rfc3339(),
                    "intent": trade_intent_label(allowed_intent),
                    "price": accepted_order.price,
                    "quantity": accepted_order.quantity,
                });
                engine.append_runtime_event(
                    "order",
                    instance_id,
                    "order.filled",
                    order_filled_payload.to_string(),
                    trace_id,
                )?;
                related_events.push(trace_event("order", "order.filled", order_filled_payload));
                let position_updated_payload = json!({
                    "symbol": symbol,
                    "bar_timestamp": bar.timestamp.to_rfc3339(),
                    "has_position": next_has_position,
                });
                engine.append_runtime_event(
                    "position",
                    instance_id,
                    "position.updated",
                    position_updated_payload.to_string(),
                    trace_id,
                )?;
                related_events.push(trace_event(
                    "position",
                    "position.updated",
                    position_updated_payload,
                ));

                return Ok((engine.allowed_risk(), Some(accepted_order), related_events));
            }

            Ok((engine.allowed_risk(), None, related_events))
        }
        RiskDecision::Reject { reason } => {
            engine.increment_risk_rejects_total();
            let rejected_payload = json!({
                "symbol": symbol,
                "bar_timestamp": bar.timestamp.to_rfc3339(),
                "intent": trade_intent_label(intent),
                "decision": "rejected",
                "reason": reason,
            });
            engine.append_runtime_event(
                "risk",
                instance_id,
                "risk.rejected",
                rejected_payload.to_string(),
                trace_id,
            )?;
            related_events.push(trace_event("risk", "risk.rejected", rejected_payload));
            Ok((
                engine.rejected_risk((*reason).to_owned()),
                None,
                related_events,
            ))
        }
    }
}

/// Applies lane fill-state effects and returns the resulting cycle position step.
///
/// # Errors
///
/// Propagates engine errors from lane-state mutation, persistence, or ledger sync.
#[allow(clippy::too_many_arguments)]
pub fn apply_process_bar_state_effects<E: LaneExecutionEngine>(
    engine: &mut E,
    instance_id: &str,
    account_id: &str,
    bar: &OhlcvBar,
    has_position_before: bool,
    next_has_position: bool,
    order_ledger_outcome: Option<OrderLedgerOutcome>,
    risk_decision: &RiskDecision,
    accepted_order: Option<&AcceptedOrder>,
    trace_id: &str,
) -> Result<PositionStep, E::Error> {
    let mutation = engine.apply_fill_state_mutation(
        instance_id,
        bar,
        next_has_position,
        accepted_order,
        order_ledger_outcome,
        risk_decision,
    )?;

    let wrote_position_record = mutation.position_record.is_some();
    if let Some(position_record) = mutation.position_record {
        engine.append_position_record(instance_id, position_record, bar, trace_id)?;
    }

    if let Some(released_notional_usd) = mutation.released_notional_usd {
        let owner = engine.ledger_owner_path_for_instance(instance_id)?;
        engine.release_position(account_id, &owner, released_notional_usd)?;
    }

    engine.sync_account_ledger_from_instances(account_id)?;

    let position_record = engine.position_record_state(instance_id)?;
    Ok(PositionStep {
        has_position_before,
        has_position_after: position_record.has_position,
        quantity_after: position_record.quantity,
        entry_price_after: position_record.entry_price,
        realized_pnl_usd: position_record.realized_pnl_usd,
        reason: wrote_position_record.then(|| "order_filled".to_owned()),
    })
}
