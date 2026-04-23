use openticker_core::{IndicatorSignal, SignalMetadata, SignalPhase, TradeIntent};
use openticker_trace::{
    BudgetRoomContext, CapitalState, CycleOutcome, CycleRiskDecisionLabel, CycleTrace,
    CycleTraceSummary, CycleTrigger, CycleTriggerKind, ExecutionFillStep, ExecutionOrderStep,
    ExecutionStep, IntentStep, PositionStep, ReconciliationContext, ReconciliationSnapshot,
    RelatedEvent, RelatedRecord, RiskStep, SignalStep, StaleDataDiagnostics, TraceIdentity,
    build_cycle_summary,
};
use serde_json::json;

#[test]
fn trace_summary_serde_roundtrip_is_stable() {
    let identity = TraceIdentity::with_trace_id(
        "trc_summary_contract",
        "aapl",
        "AAPL",
        "2026-01-01T00:00:00Z",
        SignalPhase::Confirmed,
        CycleTriggerKind::MarketBar,
    );
    let summary = build_cycle_summary(
        &identity,
        IndicatorSignal::BuyConfirmed,
        TradeIntent::OpenLong,
        CycleRiskDecisionLabel::Allowed,
        CycleOutcome::AcceptedFilled,
        1_700_000_000_000,
    );

    let serialized = serde_json::to_value(&summary).expect("summary should serialize");
    assert_eq!(serialized["trace_id"], "trc_summary_contract");
    assert_eq!(serialized["bot_id"], "aapl");
    assert_eq!(serialized["symbol"], "AAPL");
    assert_eq!(serialized["phase"], "confirmed");
    assert_eq!(serialized["trigger_kind"], "market_bar");
    assert_eq!(serialized["signal"], "buy_confirmed");
    assert_eq!(serialized["intent"], "open_long");
    assert_eq!(serialized["risk_decision"], "allowed");
    assert_eq!(serialized["outcome"], "accepted_filled");

    let roundtrip: CycleTraceSummary =
        serde_json::from_value(serialized).expect("summary should deserialize");
    assert_eq!(roundtrip, summary);
}

#[test]
fn trace_id_generation_is_deterministic_for_same_inputs() {
    let first = TraceIdentity::with_trace_id(
        "trc_deterministic",
        "aapl",
        "AAPL",
        "2026-01-01T00:00:00Z",
        SignalPhase::Preview,
        CycleTriggerKind::ManualSignal,
    );
    let second = TraceIdentity::with_trace_id(
        "trc_deterministic",
        "aapl",
        "AAPL",
        "2026-01-01T00:00:00Z",
        SignalPhase::Preview,
        CycleTriggerKind::ManualSignal,
    );

    assert_eq!(first, second);
    assert_eq!(first.trace_id, second.trace_id);
    assert_eq!(first.phase, SignalPhase::Preview);
    assert_eq!(first.trigger_kind, CycleTriggerKind::ManualSignal);
}

#[test]
fn optional_trace_fields_serialize_consistently() {
    let minimal = sample_trace(false);
    let minimal_json = serde_json::to_value(&minimal).expect("minimal trace should serialize");
    assert!(
        minimal_json["signal_step"].get("metadata").is_none(),
        "empty metadata should be omitted"
    );
    assert!(
        minimal_json["intent_step"]
            .get("strategy_rationale")
            .is_none(),
        "empty strategy rationale should be omitted"
    );
    assert!(
        minimal_json["risk_step"].get("reason").is_none(),
        "empty risk reason should be omitted"
    );
    assert!(
        minimal_json["risk_step"]
            .get("stale_data_diagnostics")
            .is_none(),
        "empty stale diagnostics should be omitted"
    );
    assert!(
        minimal_json["execution_step"].get("order").is_none(),
        "missing execution order should be omitted"
    );
    assert!(
        minimal_json["execution_step"].get("fill").is_none(),
        "missing execution fill should be omitted"
    );
    assert!(
        minimal_json["position_step"]
            .get("entry_price_after")
            .is_none(),
        "empty entry price should be omitted"
    );
    assert!(
        minimal_json["reconciliation_context"]
            .get("latest")
            .is_none(),
        "empty reconciliation context should be omitted"
    );
    assert!(
        minimal_json.get("related_records").is_none(),
        "empty related records should be omitted"
    );
    assert!(
        minimal_json.get("related_events").is_none(),
        "empty related events should be omitted"
    );

    let enriched = sample_trace(true);
    let enriched_json = serde_json::to_value(&enriched).expect("enriched trace should serialize");
    assert!(
        enriched_json["signal_step"].get("metadata").is_some(),
        "populated metadata should be serialized"
    );
    assert!(
        enriched_json["intent_step"]
            .get("strategy_rationale")
            .is_some(),
        "populated strategy rationale should be serialized"
    );
    assert!(
        enriched_json["risk_step"].get("reason").is_some(),
        "populated risk reason should be serialized"
    );
    assert!(
        enriched_json["risk_step"]
            .get("stale_data_diagnostics")
            .is_some(),
        "populated stale diagnostics should be serialized"
    );
    assert!(
        enriched_json["execution_step"].get("order").is_some(),
        "execution order should be serialized"
    );
    assert!(
        enriched_json["execution_step"].get("fill").is_some(),
        "execution fill should be serialized"
    );
    assert!(
        enriched_json["position_step"]
            .get("entry_price_after")
            .is_some(),
        "entry price should be serialized"
    );
    assert!(
        enriched_json["reconciliation_context"]
            .get("latest")
            .is_some(),
        "reconciliation context should be serialized"
    );
    assert!(
        enriched_json.get("related_records").is_some(),
        "related records should be serialized"
    );
    assert!(
        enriched_json.get("related_events").is_some(),
        "related events should be serialized"
    );
}

fn sample_trace(with_optional: bool) -> CycleTrace {
    let identity = sample_identity();
    CycleTrace {
        summary: sample_summary(&identity),
        trigger: sample_trigger(),
        signal_step: sample_signal_step(with_optional),
        intent_step: sample_intent_step(with_optional),
        risk_step: sample_risk_step(with_optional),
        execution_step: sample_execution_step(with_optional),
        position_step: sample_position_step(with_optional),
        capital_before: CapitalState::default(),
        capital_after: CapitalState::default(),
        reconciliation_context: sample_reconciliation_context(with_optional),
        related_records: sample_related_records(with_optional),
        related_events: sample_related_events(with_optional),
    }
}

fn sample_identity() -> TraceIdentity {
    TraceIdentity::with_trace_id(
        "trc_detail_contract",
        "aapl",
        "AAPL",
        "2026-01-01T00:00:00Z",
        SignalPhase::Confirmed,
        CycleTriggerKind::MarketBar,
    )
}

fn sample_summary(identity: &TraceIdentity) -> CycleTraceSummary {
    build_cycle_summary(
        identity,
        IndicatorSignal::BuyConfirmed,
        TradeIntent::OpenLong,
        CycleRiskDecisionLabel::Allowed,
        CycleOutcome::AcceptedFilled,
        1_700_000_000_000,
    )
}

fn sample_trigger() -> CycleTrigger {
    CycleTrigger {
        trigger_kind: CycleTriggerKind::MarketBar,
        phase: SignalPhase::Confirmed,
        bar_timestamp: "2026-01-01T00:00:00Z".to_owned(),
        close: 123.45,
        signal_source: "sma_crossover".to_owned(),
    }
}

fn sample_signal_step(with_optional: bool) -> SignalStep {
    SignalStep {
        signal: IndicatorSignal::BuyConfirmed,
        close: 123.45,
        metadata: with_optional.then(|| SignalMetadata {
            reason_code: Some("test_reason".to_owned()),
            ..SignalMetadata::default()
        }),
    }
}

fn sample_intent_step(with_optional: bool) -> IntentStep {
    IntentStep {
        signal: IndicatorSignal::BuyConfirmed,
        intent: TradeIntent::OpenLong,
        strategy_rationale: with_optional.then(|| "alignment".to_owned()),
        has_position_before: false,
        order_quantity: 1.0,
        order_quantity_adjustment_reason: with_optional.then(|| "budget_cap".to_owned()),
        order_ledger_outcome: with_optional.then(|| "reserved".to_owned()),
    }
}

fn sample_risk_step(with_optional: bool) -> RiskStep {
    RiskStep {
        intent: TradeIntent::OpenLong,
        decision: CycleRiskDecisionLabel::Allowed,
        reason: with_optional.then(|| "within_limits".to_owned()),
        stale_data: false,
        stale_data_diagnostics: with_optional.then_some(StaleDataDiagnostics {
            bar_timestamp_ms: 1_767_225_600_000,
            close_timestamp_ms: 1_767_229_200_000,
            stale_deadline_ms: 1_767_229_203_000,
            evaluated_at_ms: 1_767_229_201_000,
        }),
        cooldown_active: false,
        account_open_positions: 0,
        account_daily_loss_pct: 0.0,
        observed_spread_bps: 1,
        estimated_slippage_bps: 1,
        budget_room: BudgetRoomContext {
            remaining_bot_usd: 900.0,
            remaining_account_usd: 19_000.0,
        },
    }
}

fn sample_execution_step(with_optional: bool) -> ExecutionStep {
    if with_optional {
        ExecutionStep {
            order: Some(ExecutionOrderStep {
                client_order_id: "order-1".to_owned(),
                intent: TradeIntent::OpenLong,
                status: "filled".to_owned(),
                price: 123.45,
                quantity: 1.0,
            }),
            fill: Some(ExecutionFillStep {
                client_order_id: "order-1".to_owned(),
                price: 123.45,
                quantity: 1.0,
                fee_asset: Some("USD".to_owned()),
                fee_amount: Some(0.12),
                fee_normalized_usd: Some(0.12),
            }),
        }
    } else {
        ExecutionStep::default()
    }
}

fn sample_position_step(with_optional: bool) -> PositionStep {
    PositionStep {
        has_position_before: false,
        has_position_after: true,
        quantity_after: 1.0,
        entry_price_after: with_optional.then_some(123.45),
        realized_pnl_usd: 0.0,
        reason: with_optional.then(|| "opened".to_owned()),
    }
}

fn sample_reconciliation_context(with_optional: bool) -> ReconciliationContext {
    if with_optional {
        ReconciliationContext {
            latest: Some(ReconciliationSnapshot {
                source: "manual".to_owned(),
                safe_to_trade: true,
                local_open_orders: 0,
                connector_open_orders: 0,
                local_has_position: true,
                connector_has_position: true,
                reason: "aligned".to_owned(),
                created_at_ms: 1_700_000_000_001,
            }),
        }
    } else {
        ReconciliationContext::default()
    }
}

fn sample_related_records(with_optional: bool) -> Vec<RelatedRecord> {
    if with_optional {
        vec![RelatedRecord {
            family: "orders".to_owned(),
            label: "primary".to_owned(),
            bar_timestamp: Some("2026-01-01T00:00:00Z".to_owned()),
            client_order_id: Some("order-1".to_owned()),
        }]
    } else {
        Vec::new()
    }
}

fn sample_related_events(with_optional: bool) -> Vec<RelatedEvent> {
    if with_optional {
        vec![RelatedEvent {
            scope: "runtime".to_owned(),
            kind: "order.submitted".to_owned(),
            payload: json!({"client_order_id": "order-1"}),
        }]
    } else {
        Vec::new()
    }
}
