use crate::signals::{ProcessBarEvaluation, StrategySignalSource};
use openticker_core::{IndicatorSignal, OhlcvBar, SignalPhase, TradeIntent};
use openticker_execution::{AcceptedOrder, OrderLedgerOutcome};
use openticker_risk::RiskDecision;
use openticker_trace::{
    CapitalState, CycleOutcome, CycleRiskDecisionLabel, CycleTrace, CycleTrigger,
    ExecutionFillStep, ExecutionOrderStep, ExecutionStep, IntentStep, PositionStep,
    ReconciliationContext, RelatedEvent, RelatedRecord, RiskStep, SignalStep, TraceIdentity,
    build_cycle_summary,
};
use serde_json::Value;

/// Builds the final cycle trace payload for a completed lane evaluation.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
///
/// # Panics
///
/// Panics if enum labels used in the trace cannot be serialized.
#[must_use]
pub fn build_cycle_trace(
    trace: &TraceIdentity,
    bar: &OhlcvBar,
    evaluation: &ProcessBarEvaluation,
    accepted_order: Option<&AcceptedOrder>,
    position_step: PositionStep,
    capital_before: CapitalState,
    capital_after: CapitalState,
    reconciliation_context: ReconciliationContext,
    related_events: Vec<RelatedEvent>,
    created_at_ms: i64,
) -> CycleTrace {
    let outcome = cycle_outcome(evaluation, accepted_order);
    let risk_decision = cycle_risk_decision_label(&evaluation.risk_decision);
    let summary = build_cycle_summary(
        trace,
        evaluation.signal,
        evaluation.intent,
        risk_decision,
        outcome,
        created_at_ms,
    );

    let mut related_records = vec![
        RelatedRecord {
            family: "signal".to_owned(),
            label: serde_json::to_string(&evaluation.signal)
                .expect("indicator signal should serialize")
                .trim_matches('"')
                .to_owned(),
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: None,
        },
        RelatedRecord {
            family: "intent".to_owned(),
            label: serde_json::to_string(&evaluation.intent)
                .expect("trade intent should serialize")
                .trim_matches('"')
                .to_owned(),
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: None,
        },
        RelatedRecord {
            family: "risk_decision".to_owned(),
            label: serde_json::to_string(&risk_decision)
                .expect("risk decision label should serialize")
                .trim_matches('"')
                .to_owned(),
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: None,
        },
    ];
    if let Some(order) = accepted_order {
        related_records.push(RelatedRecord {
            family: "order".to_owned(),
            label: order.client_order_id.clone(),
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: Some(order.client_order_id.clone()),
        });
        related_records.push(RelatedRecord {
            family: "fill".to_owned(),
            label: order.client_order_id.clone(),
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: Some(order.client_order_id.clone()),
        });
    }
    if let Some(reason) = position_step.reason.clone() {
        related_records.push(RelatedRecord {
            family: "position".to_owned(),
            label: reason,
            bar_timestamp: Some(trace.bar_timestamp.clone()),
            client_order_id: None,
        });
    }
    if let Some(latest) = &reconciliation_context.latest {
        related_records.push(RelatedRecord {
            family: "reconciliation".to_owned(),
            label: latest.reason.clone(),
            bar_timestamp: None,
            client_order_id: None,
        });
    }

    CycleTrace {
        summary,
        trigger: CycleTrigger {
            trigger_kind: trace.trigger_kind,
            phase: trace.phase,
            bar_timestamp: trace.bar_timestamp.clone(),
            close: bar.close,
            signal_source: strategy_signal_source_label(evaluation.signal_source).to_owned(),
        },
        signal_step: SignalStep {
            signal: evaluation.signal,
            close: bar.close,
            metadata: (!evaluation.signal_metadata.is_empty())
                .then_some(evaluation.signal_metadata.clone()),
        },
        intent_step: IntentStep {
            signal: evaluation.signal,
            intent: evaluation.intent,
            strategy_rationale: evaluation.strategy_rationale.clone(),
            has_position_before: evaluation.has_position_before,
            order_quantity: evaluation.order_quantity,
            order_quantity_adjustment_reason: evaluation.order_quantity_adjustment_reason.clone(),
            order_ledger_outcome: evaluation.order_ledger_outcome.map(|value| match value {
                OrderLedgerOutcome::BotExhausted => "bot_exhausted".to_owned(),
                OrderLedgerOutcome::AccountExhausted => "account_exhausted".to_owned(),
                OrderLedgerOutcome::Dust => "dust".to_owned(),
            }),
        },
        risk_step: RiskStep {
            intent: evaluation.intent,
            decision: risk_decision,
            reason: match &evaluation.risk_decision {
                RiskDecision::Allow(_) => None,
                RiskDecision::Reject { reason } => Some((*reason).to_owned()),
            },
            stale_data: evaluation.stale_data,
            stale_data_diagnostics: evaluation.stale_data_diagnostics.clone(),
            cooldown_active: evaluation.cooldown_active,
            account_open_positions: evaluation.account_open_positions,
            account_daily_loss_pct: evaluation.account_daily_loss_pct,
            observed_spread_bps: u64::from(evaluation.observed_spread_bps),
            estimated_slippage_bps: u64::from(evaluation.estimated_slippage_bps),
            budget_room: evaluation.budget_room.clone(),
        },
        execution_step: ExecutionStep {
            order: accepted_order.map(|order| ExecutionOrderStep {
                client_order_id: order.client_order_id.clone(),
                intent: evaluation.intent,
                status: "submitted".to_owned(),
                price: order.price,
                quantity: order.quantity,
            }),
            fill: accepted_order.map(|order| ExecutionFillStep {
                client_order_id: order.client_order_id.clone(),
                price: order.price,
                quantity: order.quantity,
                fee_asset: order.fee_asset.clone(),
                fee_amount: order.fee_amount,
                fee_normalized_usd: order.fee_normalized_usd,
            }),
        },
        position_step,
        capital_before,
        capital_after,
        reconciliation_context,
        related_records,
        related_events,
    }
}

pub(crate) fn signal_phase_label(phase: SignalPhase) -> &'static str {
    match phase {
        SignalPhase::Preview => "preview",
        SignalPhase::Confirmed => "confirmed",
    }
}

pub(crate) fn indicator_signal_label(signal: IndicatorSignal) -> &'static str {
    match signal {
        IndicatorSignal::None => "none",
        IndicatorSignal::BuyPreview => "buy_preview",
        IndicatorSignal::BuyConfirmed => "buy_confirmed",
        IndicatorSignal::SellPreview => "sell_preview",
        IndicatorSignal::SellConfirmed => "sell_confirmed",
    }
}

pub(crate) fn strategy_signal_source_label(source: StrategySignalSource) -> &'static str {
    match source {
        StrategySignalSource::Representative => "representative",
        StrategySignalSource::IntentFallback => "intent_fallback",
        StrategySignalSource::Manual => "manual",
    }
}

pub(crate) fn trade_intent_label(intent: TradeIntent) -> &'static str {
    match intent {
        TradeIntent::NoOp => "no_op",
        TradeIntent::OpenLong => "open_long",
        TradeIntent::AddLong => "add_long",
        TradeIntent::ReduceLong => "reduce_long",
        TradeIntent::CloseLong => "close_long",
    }
}

pub(crate) fn ledger_outcome_reason_code(outcome: OrderLedgerOutcome) -> &'static str {
    match outcome {
        OrderLedgerOutcome::BotExhausted => "bot_ledger_exhausted",
        OrderLedgerOutcome::AccountExhausted => "account_ledger_exhausted",
        OrderLedgerOutcome::Dust => "ledger_dust",
    }
}

pub(crate) fn trace_event(scope: &str, kind: &str, payload: Value) -> RelatedEvent {
    RelatedEvent {
        scope: scope.to_owned(),
        kind: kind.to_owned(),
        payload,
    }
}

fn cycle_risk_decision_label(decision: &RiskDecision) -> CycleRiskDecisionLabel {
    match decision {
        RiskDecision::Allow(_) => CycleRiskDecisionLabel::Allowed,
        RiskDecision::Reject { .. } => CycleRiskDecisionLabel::Rejected,
    }
}

fn cycle_outcome(
    evaluation: &ProcessBarEvaluation,
    accepted_order: Option<&AcceptedOrder>,
) -> CycleOutcome {
    match &evaluation.risk_decision {
        RiskDecision::Reject { .. } => CycleOutcome::RiskRejected,
        RiskDecision::Allow(intent) => {
            if matches!(intent, TradeIntent::NoOp) && accepted_order.is_none() {
                CycleOutcome::NoOp
            } else if let Some(order) = accepted_order {
                if order.quantity + f64::EPSILON < evaluation.order_quantity {
                    CycleOutcome::AcceptedPartiallyFilled
                } else {
                    CycleOutcome::AcceptedFilled
                }
            } else {
                CycleOutcome::AcceptedNoFill
            }
        }
    }
}
