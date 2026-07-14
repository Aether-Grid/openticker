use crate::position::effective_position_quantity;
use crate::state::LaneRuntime;
use openticker_config::ExecutionConstraintsConfig;
use openticker_core::{
    IndicatorMetadataCapabilities, IndicatorRole, IndicatorSignal, IndicatorSignalMetadataFilters,
    IndicatorSignalPolicy, MarketType, OhlcvBar, SignalMetadata, SignalPhase, Timeframe,
    TradeIntent,
};
use openticker_execution::OrderLedgerOutcome;
use openticker_instance::{
    EvaluatedIndicatorSignal, RuntimeStrategyEngine, default_signal_policy,
    representative_indicator,
};
use openticker_ledger::calculate_position_notional_usd;
use openticker_risk::{BasicRiskPolicy, RiskContext, RiskDecision, RiskLimits, RiskPolicy};
use openticker_strategy::{
    ConsensusStrategy, ConsensusStrategyContext, IndicatorObservation, Strategy, StrategyContext,
};
use openticker_trace::{BudgetRoomContext, StaleDataDiagnostics};

#[must_use]
pub fn apply_position_transition(has_position: bool, intent: TradeIntent) -> bool {
    match intent {
        TradeIntent::OpenLong | TradeIntent::AddLong => true,
        TradeIntent::ReduceLong | TradeIntent::CloseLong => false,
        TradeIntent::NoOp => has_position,
    }
}

#[must_use]
pub fn resolved_strategy_signal(
    representative_signal: IndicatorSignal,
    intent: TradeIntent,
    phase: SignalPhase,
) -> (IndicatorSignal, StrategySignalSource) {
    if representative_signal != IndicatorSignal::None {
        return (representative_signal, StrategySignalSource::Representative);
    }

    let signal = match intent {
        TradeIntent::OpenLong | TradeIntent::AddLong => match phase {
            SignalPhase::Preview => IndicatorSignal::BuyPreview,
            SignalPhase::Confirmed => IndicatorSignal::BuyConfirmed,
        },
        TradeIntent::ReduceLong | TradeIntent::CloseLong => match phase {
            SignalPhase::Preview => IndicatorSignal::SellPreview,
            SignalPhase::Confirmed => IndicatorSignal::SellConfirmed,
        },
        TradeIntent::NoOp => IndicatorSignal::None,
    };
    let source = if signal == IndicatorSignal::None {
        StrategySignalSource::Representative
    } else {
        StrategySignalSource::IntentFallback
    };

    (signal, source)
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProcessBarEvaluation {
    pub signal: IndicatorSignal,
    pub signal_metadata: SignalMetadata,
    pub signal_source: StrategySignalSource,
    pub intent: TradeIntent,
    pub strategy_rationale: Option<String>,
    pub order_quantity: f64,
    pub order_quantity_adjustment_reason: Option<String>,
    pub order_ledger_outcome: Option<OrderLedgerOutcome>,
    pub risk_decision: RiskDecision,
    pub stale_data: bool,
    pub stale_data_diagnostics: Option<StaleDataDiagnostics>,
    pub cooldown_active: bool,
    pub account_open_positions: u32,
    pub account_daily_loss_pct: f64,
    pub observed_spread_bps: u32,
    pub estimated_slippage_bps: u32,
    pub budget_room: BudgetRoomContext,
    pub has_position_before: bool,
    pub next_has_position: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedLaneEvaluation {
    pub bot_id: String,
    pub account_id: String,
    pub market: MarketType,
    pub bot_budget_pct: f64,
    pub timeframe: Timeframe,
    pub risk_limits: RiskLimits,
    pub target_order_notional_usd: f64,
    pub current_position_quantity: f64,
    pub has_position_before: bool,
    pub cooldown_active: bool,
    pub instance_execution_constraints: ExecutionConstraintsConfig,
    pub connector_execution_constraints: Option<ExecutionConstraintsConfig>,
    pub connector_fractional_entry_supported: Option<bool>,
    pub signal: IndicatorSignal,
    pub signal_metadata: SignalMetadata,
    pub signal_source: StrategySignalSource,
    pub intent: TradeIntent,
    pub strategy_rationale: Option<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct SignalEvaluationKernelInput {
    pub signal: IndicatorSignal,
    pub signal_metadata: SignalMetadata,
    pub signal_source: StrategySignalSource,
    pub intent: TradeIntent,
    pub strategy_rationale: Option<String>,
    pub bar_close: f64,
    pub stale_data: bool,
    pub stale_data_diagnostics: Option<StaleDataDiagnostics>,
    pub account_open_positions: u32,
    pub account_daily_loss_pct: f64,
    pub risk_limits: RiskLimits,
    pub order_quantity_resolution: openticker_execution::OrderQuantityResolution,
    pub budget_room: BudgetRoomContext,
    pub kill_switch_active: bool,
    pub has_position_before: bool,
    pub cooldown_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategySignalSource {
    Representative,
    IntentFallback,
    Manual,
}

#[must_use]
pub fn evaluate_indicator_signals(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> Vec<EvaluatedIndicatorSignal> {
    openticker_instance::evaluate_indicator_signals(&mut instance.indicators, bar, phase)
}

#[must_use]
pub fn prepare_process_bar_evaluation(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> PreparedLaneEvaluation {
    let evaluated_signals = evaluate_indicator_signals(instance, bar, phase);
    let representative_indicator = representative_indicator(&evaluated_signals);
    let representative_signal =
        representative_indicator.map_or(IndicatorSignal::None, |indicator| indicator.signal);
    let representative_signal_policy = representative_indicator.map_or(
        default_signal_policy(instance.config.signal_mode),
        |indicator| indicator.signal_policy,
    );
    let representative_indicator_id = representative_indicator
        .map_or("representative_fallback", |indicator| indicator.id.as_str());
    let representative_metadata_capabilities = representative_indicator
        .map_or(IndicatorMetadataCapabilities::default(), |indicator| {
            indicator.metadata_capabilities
        });
    let representative_metadata_filters = representative_indicator
        .map_or_else(IndicatorSignalMetadataFilters::default, |indicator| {
            indicator.metadata_filters.clone()
        });
    let representative_metadata = representative_indicator
        .map_or_else(SignalMetadata::default, |indicator| {
            indicator.metadata.clone()
        });
    let has_position_before = instance.has_position;
    let current_position_quantity = effective_position_quantity(instance);
    instance.position_notional_usd =
        calculate_position_notional_usd(current_position_quantity, bar.close);

    let strategy_decision = match &mut instance.strategy {
        RuntimeStrategyEngine::Single(strategy) => strategy.decide(StrategyContext {
            indicator_id: representative_indicator_id,
            signal: representative_signal,
            signal_policy: representative_signal_policy,
            metadata_capabilities: representative_metadata_capabilities,
            metadata_filters: &representative_metadata_filters,
            metadata: &representative_metadata,
            has_position: has_position_before,
        }),
        RuntimeStrategyEngine::Consensus(strategy) => {
            let consensus_observations = evaluated_signals
                .iter()
                .map(|indicator| IndicatorObservation {
                    id: &indicator.id,
                    role: indicator.role,
                    signal_policy: indicator.signal_policy,
                    signal: indicator.signal,
                    metadata_capabilities: indicator.metadata_capabilities,
                    metadata_filters: &indicator.metadata_filters,
                    metadata: &indicator.metadata,
                    weight: indicator.weight,
                })
                .collect::<Vec<_>>();

            strategy.decide_consensus(ConsensusStrategyContext {
                indicators: &consensus_observations,
                has_position: has_position_before,
            })
        }
    };
    let (signal, signal_source) =
        resolved_strategy_signal(representative_signal, strategy_decision.intent, phase);
    let cooldown_active = instance
        .cooldown_until_ms
        .is_some_and(|until_ms| bar.timestamp.timestamp_millis() < until_ms);

    PreparedLaneEvaluation {
        bot_id: instance.config.id.clone(),
        account_id: instance.config.account.clone(),
        market: instance.config.market,
        bot_budget_pct: instance.config.budget.pct,
        timeframe: instance.config.timeframe,
        risk_limits: instance.risk_limits,
        target_order_notional_usd: instance.target_order_notional_usd,
        current_position_quantity,
        has_position_before,
        cooldown_active,
        instance_execution_constraints: instance.config.execution_constraints.clone(),
        connector_execution_constraints: instance.connector_execution_constraints.clone(),
        connector_fractional_entry_supported: instance.connector_fractional_entry_supported,
        signal,
        signal_metadata: representative_metadata,
        signal_source,
        intent: strategy_decision.intent,
        strategy_rationale: strategy_decision.rationale,
    }
}

#[must_use]
pub fn prepare_manual_signal_evaluation(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    signal: IndicatorSignal,
) -> PreparedLaneEvaluation {
    let has_position_before = instance.has_position;
    let current_position_quantity = effective_position_quantity(instance);
    instance.position_notional_usd =
        calculate_position_notional_usd(current_position_quantity, bar.close);

    let signal_policy = IndicatorSignalPolicy::PreviewAllowed;
    let metadata_capabilities = IndicatorMetadataCapabilities::default();
    let metadata_filters = IndicatorSignalMetadataFilters::default();
    let metadata = SignalMetadata::default();
    let strategy_decision = match &mut instance.strategy {
        RuntimeStrategyEngine::Single(strategy) => strategy.decide(StrategyContext {
            indicator_id: "manual_signal",
            signal,
            signal_policy,
            metadata_capabilities,
            metadata_filters: &metadata_filters,
            metadata: &metadata,
            has_position: has_position_before,
        }),
        RuntimeStrategyEngine::Consensus(strategy) => {
            let synthetic_observation = [IndicatorObservation {
                id: "manual_signal",
                role: IndicatorRole::PrimarySignal,
                signal_policy,
                signal,
                metadata_capabilities,
                metadata_filters: &metadata_filters,
                metadata: &metadata,
                weight: 1.0,
            }];
            strategy.decide_consensus(ConsensusStrategyContext {
                indicators: &synthetic_observation,
                has_position: has_position_before,
            })
        }
    };
    let cooldown_active = instance
        .cooldown_until_ms
        .is_some_and(|until_ms| bar.timestamp.timestamp_millis() < until_ms);

    PreparedLaneEvaluation {
        bot_id: instance.config.id.clone(),
        account_id: instance.config.account.clone(),
        market: instance.config.market,
        bot_budget_pct: instance.config.budget.pct,
        timeframe: instance.config.timeframe,
        risk_limits: instance.risk_limits,
        target_order_notional_usd: instance.target_order_notional_usd,
        current_position_quantity,
        has_position_before,
        cooldown_active,
        instance_execution_constraints: instance.config.execution_constraints.clone(),
        connector_execution_constraints: instance.connector_execution_constraints.clone(),
        connector_fractional_entry_supported: instance.connector_fractional_entry_supported,
        signal,
        signal_metadata: SignalMetadata::default(),
        signal_source: StrategySignalSource::Manual,
        intent: strategy_decision.intent,
        strategy_rationale: strategy_decision.rationale,
    }
}

#[must_use]
pub fn apply_state_only_confirmed_bar(instance: &mut LaneRuntime, bar: &OhlcvBar) -> bool {
    if instance
        .last_dispatched_bar_timestamp
        .is_some_and(|previous| previous >= bar.timestamp)
    {
        return false;
    }

    for indicator in &mut instance.indicators {
        let _ = indicator.engine.evaluate(bar, SignalPhase::Confirmed);
    }

    instance.last_dispatched_bar_timestamp = Some(bar.timestamp);
    true
}

#[must_use]
pub fn build_process_bar_evaluation(input: SignalEvaluationKernelInput) -> ProcessBarEvaluation {
    let SignalEvaluationKernelInput {
        signal,
        signal_metadata,
        signal_source,
        intent,
        strategy_rationale,
        bar_close,
        stale_data,
        stale_data_diagnostics,
        account_open_positions,
        account_daily_loss_pct,
        risk_limits,
        order_quantity_resolution,
        budget_room,
        kill_switch_active,
        has_position_before,
        cooldown_active,
    } = input;

    let order_quantity = order_quantity_resolution.quantity;
    // Quote- and book-derived quality inputs are still unavailable on LaneRuntime.
    let observed_spread_bps = 0_u32;
    let estimated_slippage_bps = 0_u32;
    let risk_policy = BasicRiskPolicy {
        limits: risk_limits,
        kill_switch_active,
    };
    let mut risk_decision = risk_policy.evaluate(RiskContext {
        intent,
        price: bar_close,
        quantity: order_quantity,
        stale_data,
        account_open_positions,
        account_daily_loss_pct,
        observed_spread_bps,
        estimated_slippage_bps,
        cooldown_active,
    });

    match order_quantity_resolution.ledger_outcome {
        Some(OrderLedgerOutcome::BotExhausted) => {
            risk_decision = RiskDecision::Reject {
                reason: "bot ledger exhausted",
            };
        }
        Some(OrderLedgerOutcome::AccountExhausted) => {
            risk_decision = RiskDecision::Reject {
                reason: "account ledger exhausted",
            };
        }
        Some(OrderLedgerOutcome::Dust)
            if matches!(intent, TradeIntent::OpenLong | TradeIntent::AddLong) =>
        {
            risk_decision = RiskDecision::Allow(TradeIntent::NoOp);
        }
        None | Some(OrderLedgerOutcome::Dust) => {}
    }

    let next_has_position = match risk_decision {
        RiskDecision::Allow(allowed_intent) => {
            apply_position_transition(has_position_before, allowed_intent)
        }
        RiskDecision::Reject { .. } => has_position_before,
    };

    ProcessBarEvaluation {
        signal,
        signal_metadata,
        signal_source,
        intent,
        strategy_rationale,
        order_quantity,
        order_quantity_adjustment_reason: order_quantity_resolution.adjustment_reason,
        order_ledger_outcome: order_quantity_resolution.ledger_outcome,
        risk_decision,
        stale_data,
        stale_data_diagnostics,
        cooldown_active,
        account_open_positions,
        account_daily_loss_pct,
        observed_spread_bps,
        estimated_slippage_bps,
        budget_room,
        has_position_before,
        next_has_position,
    }
}

#[must_use]
pub fn market_data_freshness(
    bar: &OhlcvBar,
    timeframe: Timeframe,
    stale_data_ms: u64,
    evaluated_at_ms: i64,
) -> (bool, StaleDataDiagnostics) {
    let timeframe_ms = i64::try_from(timeframe.duration().as_millis()).unwrap_or(i64::MAX);
    let grace_ms = i64::try_from(stale_data_ms).unwrap_or(i64::MAX);
    let close_timestamp_ms = bar
        .timestamp
        .timestamp_millis()
        .saturating_add(timeframe_ms);
    let stale_deadline_ms = close_timestamp_ms.saturating_add(grace_ms);
    let diagnostics = StaleDataDiagnostics {
        bar_timestamp_ms: bar.timestamp.timestamp_millis(),
        close_timestamp_ms,
        stale_deadline_ms,
        evaluated_at_ms,
    };

    (evaluated_at_ms > stale_deadline_ms, diagnostics)
}

#[must_use]
pub fn market_data_is_stale(bar: &OhlcvBar, timeframe: Timeframe, stale_data_ms: u64) -> bool {
    market_data_freshness(
        bar,
        timeframe,
        stale_data_ms,
        chrono::Utc::now().timestamp_millis(),
    )
    .0
}
