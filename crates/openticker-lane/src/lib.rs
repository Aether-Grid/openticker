use openticker_config::{ExecutionConstraintsConfig, InstanceConfig};
use openticker_connectors::{ConfirmedBarPage, ConnectorAccountSnapshot};
use openticker_core::{
    BotLaneKey, ExecutionMode, IndicatorMetadataCapabilities, IndicatorRole, IndicatorSignal,
    IndicatorSignalMetadataFilters, IndicatorSignalPolicy, MarketType, OhlcvBar, SignalMetadata,
    SignalPhase, Timeframe, TradeIntent,
};
use openticker_data::{BarBuilder, NormalizedBarUpdate};
use openticker_execution::{AcceptedOrder, ExecutionRequest, OrderLedgerOutcome};
use openticker_instance::{
    ConfiguredIndicatorRuntime, EvaluatedIndicatorSignal, InstanceError, RuntimeStrategyEngine,
};
pub use openticker_instance::{default_signal_policy, representative_indicator};
use openticker_ledger::{
    FeeEntry, InventoryError, InventoryFillSide, InventoryState, LedgerOwnerPath, ReservationError,
    calculate_position_notional_usd, sanitize_ledger_value,
};
use openticker_risk::{BasicRiskPolicy, RiskContext, RiskDecision, RiskLimits, RiskPolicy};
use openticker_strategy::{
    ConsensusStrategy, ConsensusStrategyContext, IndicatorObservation, Strategy, StrategyContext,
};
use openticker_trace::{
    BudgetRoomContext, CapitalState, CycleOutcome, CycleRiskDecisionLabel, CycleTrace,
    CycleTrigger, CycleTriggerKind, ExecutionFillStep, ExecutionOrderStep, ExecutionStep,
    IntentStep, PositionStep, ReconciliationContext, RelatedEvent, RelatedRecord, RiskStep,
    SignalStep, StaleDataDiagnostics, TraceIdentity, build_cycle_summary,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    hash::BuildHasher,
    time::{Duration, Instant},
};

const POSITION_QUANTITY_TOLERANCE: f64 = 1e-9;

pub type RuntimeLaneBuild = (HashMap<String, LaneRuntime>, HashMap<String, Vec<String>>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRuntimeState {
    Stopped,
    Running,
    Paused,
    Reconciling,
}

impl LaneRuntimeState {
    #[must_use]
    pub fn as_storage_value(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Reconciling => "reconciling",
        }
    }

    #[must_use]
    pub fn from_storage_value(value: &str) -> Option<Self> {
        match value {
            "stopped" => Some(Self::Stopped),
            "running" => Some(Self::Running),
            "paused" => Some(Self::Paused),
            "reconciling" => Some(Self::Reconciling),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRecoveryState {
    Healthy,
    CatchingUp,
    OutOfSync,
}

#[derive(Debug, Clone)]
pub struct InstanceWarmupState {
    pub required_bars: usize,
    pub loaded_bars: usize,
    pub ready: bool,
    pub last_error: Option<String>,
    pub last_warmup_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

impl InstanceWarmupState {
    #[must_use]
    pub fn new(required_bars: usize) -> Self {
        Self {
            required_bars,
            loaded_bars: 0,
            ready: required_bars == 0,
            last_error: None,
            last_warmup_timestamp: None,
        }
    }
}

/// Computes the warmup bar target for a lane based on enabled indicators.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when an enabled indicator
/// references an unknown indicator type.
pub fn required_warmup_bars(instance: &InstanceConfig) -> Result<usize, InstanceError> {
    openticker_instance::required_warmup_bars(instance)
}

/// Builds enabled runtime indicator engines for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when no indicators are
/// enabled, an indicator type is unknown, or indicator parameters are invalid.
pub fn build_runtime_indicators(
    instance: &InstanceConfig,
) -> Result<Vec<ConfiguredIndicatorRuntime>, InstanceError> {
    openticker_instance::build_runtime_indicators(instance)
}

/// Builds the runtime strategy engine for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the configured strategy
/// is unsupported.
pub fn build_runtime_strategy(
    instance: &InstanceConfig,
) -> Result<RuntimeStrategyEngine, InstanceError> {
    openticker_instance::build_runtime_strategy(instance)
}

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

#[derive(Debug, Clone, Copy)]
pub struct RecoveredInstanceState {
    pub state: LaneRuntimeState,
    pub resume_after_startup_reconcile: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct RiskProfileRuntimeConfig {
    pub limits: RiskLimits,
    pub target_order_notional_usd: f64,
}

#[must_use]
pub fn recover_lane_state(
    recovered: LaneRuntimeState,
    default_start_paused_if_recovery_uncertain: bool,
) -> LaneRuntimeState {
    if default_start_paused_if_recovery_uncertain && matches!(recovered, LaneRuntimeState::Running)
    {
        LaneRuntimeState::Reconciling
    } else {
        recovered
    }
}

/// Resolves the lane identifier for an instance/symbol pair.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the lane identity cannot
/// be encoded for a multi-symbol bot.
pub fn lane_instance_id(instance: &InstanceConfig, symbol: &str) -> Result<String, InstanceError> {
    if instance.symbols.len() == 1 {
        Ok(instance.id.clone())
    } else {
        BotLaneKey::parse(instance.id.clone(), symbol.to_owned())
            .map(|lane_key| lane_key.encoded())
            .map_err(|error| {
                InstanceError::InvalidConfiguration(format!(
                    "invalid lane identity for instance `{}` symbol `{symbol}`: {error}",
                    instance.id
                ))
            })
    }
}

#[must_use]
pub fn resolved_instance_state(
    instance: &InstanceConfig,
    snapshot_states: &HashMap<String, String, impl BuildHasher>,
    default_start_paused_if_recovery_uncertain: bool,
) -> RecoveredInstanceState {
    let default_state = if instance.enabled {
        LaneRuntimeState::Stopped
    } else {
        LaneRuntimeState::Paused
    };

    let persisted_state = snapshot_states
        .get(&instance.id)
        .and_then(|state| LaneRuntimeState::from_storage_value(state));

    let recovered_state = match persisted_state {
        Some(parsed) => recover_lane_state(parsed, default_start_paused_if_recovery_uncertain),
        None if snapshot_states.contains_key(&instance.id)
            && default_start_paused_if_recovery_uncertain =>
        {
            LaneRuntimeState::Reconciling
        }
        None => default_state,
    };

    let state = if instance.enabled {
        recovered_state
    } else {
        LaneRuntimeState::Paused
    };

    RecoveredInstanceState {
        state,
        resume_after_startup_reconcile: instance.enabled
            && default_start_paused_if_recovery_uncertain
            && matches!(persisted_state, Some(LaneRuntimeState::Running))
            && matches!(state, LaneRuntimeState::Reconciling),
    }
}

/// Resolves effective risk limits for a lane by applying instance overrides on
/// top of the referenced risk profile.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the referenced risk
/// profile is missing.
pub fn resolved_risk_limits(
    instance: &InstanceConfig,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
) -> Result<RiskLimits, InstanceError> {
    let base_limits = risk_profiles_by_id
        .get(&instance.risk.profile)
        .map(|profile| profile.limits)
        .ok_or_else(|| {
            InstanceError::InvalidConfiguration(format!(
                "instance `{}` references unknown risk profile `{}`",
                instance.id, instance.risk.profile
            ))
        })?;

    Ok(RiskLimits {
        max_daily_loss_pct: instance
            .risk
            .overrides
            .max_daily_loss_pct
            .unwrap_or(base_limits.max_daily_loss_pct),
        max_open_positions: instance
            .risk
            .overrides
            .max_open_positions
            .unwrap_or(base_limits.max_open_positions),
        max_order_notional_usd: instance
            .risk
            .overrides
            .max_order_notional_usd
            .unwrap_or(base_limits.max_order_notional_usd),
        max_spread_bps: instance
            .risk
            .overrides
            .max_spread_bps
            .unwrap_or(base_limits.max_spread_bps),
        max_slippage_bps: instance
            .risk
            .overrides
            .max_slippage_bps
            .unwrap_or(base_limits.max_slippage_bps),
        stale_data_ms: instance
            .risk
            .overrides
            .stale_data_ms
            .unwrap_or(base_limits.stale_data_ms),
        cooldown_after_reject_ms: instance
            .risk
            .overrides
            .cooldown_after_reject_ms
            .unwrap_or(base_limits.cooldown_after_reject_ms),
    })
}

/// Resolves the effective target order notional for a lane.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the referenced risk
/// profile is missing.
pub fn resolved_target_order_notional_usd(
    instance: &InstanceConfig,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
) -> Result<f64, InstanceError> {
    let base_target = risk_profiles_by_id
        .get(&instance.risk.profile)
        .map(|profile| profile.target_order_notional_usd)
        .ok_or_else(|| {
            InstanceError::InvalidConfiguration(format!(
                "instance `{}` references unknown risk profile `{}`",
                instance.id, instance.risk.profile
            ))
        })?;

    Ok(instance
        .risk
        .overrides
        .target_order_notional_usd
        .unwrap_or(base_target))
}

/// Builds the mutable lane runtime state from config, recovery state, and
/// recovered ledger signals.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when the lane wiring cannot be
/// built from config.
pub fn build_lane_runtime(
    instance: &InstanceConfig,
    symbol: &str,
    state: LaneRuntimeState,
    resume_after_startup_reconcile: bool,
    execution_mode: ExecutionMode,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
    recovered_realized_pnl_usd: f64,
) -> Result<LaneRuntime, InstanceError> {
    let risk_limits = resolved_risk_limits(instance, risk_profiles_by_id)?;
    let target_order_notional_usd =
        resolved_target_order_notional_usd(instance, risk_profiles_by_id)?;
    let required_warmup_bars = required_warmup_bars(instance)?;
    let indicators = build_runtime_indicators(instance)?;
    let strategy = build_runtime_strategy(instance)?;

    Ok(LaneRuntime {
        config: instance.clone(),
        lane_symbol: symbol.to_owned(),
        execution_mode,
        state,
        resume_after_startup_reconcile,
        indicators,
        strategy,
        bar_builder: BarBuilder::new(symbol.to_owned(), instance.timeframe),
        risk_limits,
        target_order_notional_usd,
        inventory: inventory_state_from_runtime_fields(0.0, None, recovered_realized_pnl_usd),
        has_position: false,
        position_quantity: 0.0,
        position_notional_usd: 0.0,
        entry_price: None,
        realized_pnl_usd: recovered_realized_pnl_usd,
        daily_loss_pct_accumulated: 0.0,
        last_loss_reset_date: None,
        cooldown_until_ms: None,
        reconciliation_blocked: matches!(state, LaneRuntimeState::Reconciling),
        remote_net_qty: None,
        aggregate_managed_qty: 0.0,
        external_delta_qty: None,
        managed_remote_open_orders: 0,
        external_remote_open_orders: 0,
        warmup: InstanceWarmupState::new(required_warmup_bars),
        recovery_state: LaneRecoveryState::Healthy,
        recovery_started_at_ms: None,
        recovery_target_timestamp: None,
        recovery_last_progress_timestamp: None,
        recovery_last_error: None,
        recovery_consecutive_no_progress_cycles: 0,
        last_recovered_at_timestamp: None,
        last_dispatched_bar_timestamp: None,
        last_stream_update: None,
        connector_execution_constraints: None,
        connector_fractional_entry_supported: None,
        connector_execution_constraints_initialized: false,
    })
}

/// Builds all lane runtimes and the bot-to-lane catalog from config, recovered
/// snapshot state, and recovered realized `PnL`.
///
/// # Errors
///
/// Returns `InstanceError::InvalidConfiguration` when a lane identity cannot
/// be resolved or a lane runtime cannot be built from config.
pub fn build_runtime_lanes(
    instances: &[InstanceConfig],
    account_modes: &HashMap<String, ExecutionMode, impl BuildHasher>,
    risk_profiles_by_id: &HashMap<String, RiskProfileRuntimeConfig, impl BuildHasher>,
    snapshot_states: &HashMap<String, String, impl BuildHasher>,
    recovered_realized_pnl_by_lane: &HashMap<String, f64, impl BuildHasher>,
    default_start_paused_if_recovery_uncertain: bool,
) -> Result<RuntimeLaneBuild, InstanceError> {
    let mut runtimes = HashMap::new();
    let mut lanes_by_bot = HashMap::new();

    for instance in instances {
        let recovered = resolved_instance_state(
            instance,
            snapshot_states,
            default_start_paused_if_recovery_uncertain,
        );
        let execution_mode = account_modes
            .get(&instance.account)
            .copied()
            .unwrap_or(ExecutionMode::Paper);

        let mut lane_ids = Vec::with_capacity(instance.symbols.len());
        for symbol in &instance.symbols {
            let lane_id = lane_instance_id(instance, symbol)?;
            let recovered_realized_pnl_usd = recovered_realized_pnl_by_lane
                .get(&lane_id)
                .copied()
                .unwrap_or(0.0);
            let runtime = build_lane_runtime(
                instance,
                symbol,
                recovered.state,
                recovered.resume_after_startup_reconcile,
                execution_mode,
                risk_profiles_by_id,
                recovered_realized_pnl_usd,
            )?;
            lane_ids.push(lane_id.clone());
            runtimes.insert(lane_id, runtime);
        }

        lanes_by_bot.insert(instance.id.clone(), lane_ids);
    }

    Ok((runtimes, lanes_by_bot))
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
pub struct LaneCycleContext {
    pub bot_id: String,
    pub account_id: String,
    pub execution_connector: String,
    pub symbol: String,
    pub has_position: bool,
}

#[allow(
    clippy::missing_errors_doc,
    clippy::too_many_arguments,
    clippy::type_complexity
)]
pub trait LaneCycleEngine {
    type Error;
    type Evaluation;
    type AcceptedOrder;
    type PositionStep;
    type CapitalState;
    type RelatedEvent;
    type Risk;
    type Outcome;

    fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error>;
    fn lane_cycle_context(&self, instance_id: &str) -> Result<LaneCycleContext, Self::Error>;
    fn has_matching_cycle_trace(
        &self,
        bot_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        trigger_kind: CycleTriggerKind,
        signal: Option<IndicatorSignal>,
    ) -> Result<bool, Self::Error>;
    fn duplicate_cycle_replay_outcome(
        &self,
        bot_id: String,
        symbol: String,
        phase: SignalPhase,
        has_position: bool,
    ) -> Self::Outcome;
    fn sync_account_ledger(&mut self, account_id: &str) -> Result<(), Self::Error>;
    fn capture_cycle_capital_state(
        &self,
        account_id: &str,
        bot_id: &str,
        symbol: &str,
    ) -> Self::CapitalState;
    fn validate_account_connector_kind(
        &self,
        instance_id: &str,
        account_id: &str,
        execution_connector: &str,
    ) -> Result<(), Self::Error>;
    fn ensure_account_connector_ready(
        &self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<(), Self::Error>;
    fn ensure_connector_execution_constraints(
        &mut self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<(), Self::Error>;
    fn refresh_daily_loss_rollover(&mut self, date: chrono::NaiveDate);
    fn process_pending_warmup_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<Option<Self::Outcome>, Self::Error>;
    fn process_pending_recovery_bar(
        &mut self,
        instance_id: &str,
        phase: SignalPhase,
    ) -> Result<Option<Self::Outcome>, Self::Error>;
    fn evaluate_process_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<Self::Evaluation, Self::Error>;
    fn evaluate_manual_signal(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        signal: IndicatorSignal,
    ) -> Result<Self::Evaluation, Self::Error>;
    fn append_process_bar_records(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
        evaluation: &Self::Evaluation,
        trace_id: &str,
    ) -> Result<Vec<Self::RelatedEvent>, Self::Error>;
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
    >;
    fn apply_process_bar_state(
        &mut self,
        instance_id: &str,
        account_id: &str,
        bar: &OhlcvBar,
        evaluation: &Self::Evaluation,
        accepted_order: Option<&Self::AcceptedOrder>,
        trace_id: &str,
    ) -> Result<Self::PositionStep, Self::Error>;
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
    ) -> Result<(), Self::Error>;
    fn update_last_dispatched_bar_timestamp(
        &mut self,
        instance_id: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Self::Error>;
    fn outcome_from_evaluation(
        &self,
        instance_id: &str,
        context: &LaneCycleContext,
        phase: SignalPhase,
        evaluation: &Self::Evaluation,
        risk: Self::Risk,
    ) -> Result<Self::Outcome, Self::Error>;
    fn record_successful_cycle_latency(&mut self, elapsed: Duration);
}

#[derive(Debug, Clone, Copy)]
enum LaneCycleInvocation {
    MarketBar,
    ManualSignal { signal: IndicatorSignal },
}

impl LaneCycleInvocation {
    fn trigger_kind(self) -> CycleTriggerKind {
        match self {
            Self::MarketBar => CycleTriggerKind::MarketBar,
            Self::ManualSignal { .. } => CycleTriggerKind::ManualSignal,
        }
    }

    fn signal(self) -> Option<IndicatorSignal> {
        match self {
            Self::MarketBar => None,
            Self::ManualSignal { signal } => Some(signal),
        }
    }

    fn processes_pending_state(self) -> bool {
        matches!(self, Self::MarketBar)
    }

    fn updates_last_dispatched_bar_timestamp(self, phase: SignalPhase) -> bool {
        matches!(self, Self::MarketBar) && matches!(phase, SignalPhase::Confirmed)
    }
}

/// Resolves the effective processing phase for a manually injected signal.
#[must_use]
pub fn manual_signal_phase(signal: IndicatorSignal) -> SignalPhase {
    match signal {
        IndicatorSignal::BuyPreview | IndicatorSignal::SellPreview => SignalPhase::Preview,
        IndicatorSignal::BuyConfirmed | IndicatorSignal::SellConfirmed | IndicatorSignal::None => {
            SignalPhase::Confirmed
        }
    }
}

/// Runs the shared lane cycle workflow for a market-data bar.
///
/// # Errors
///
/// Propagates any workflow error returned by the engine implementation.
pub fn run_process_bar_cycle<E: LaneCycleEngine>(
    engine: &mut E,
    instance_id: &str,
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> Result<E::Outcome, E::Error> {
    run_lane_cycle(
        engine,
        instance_id,
        bar,
        phase,
        LaneCycleInvocation::MarketBar,
    )
}

/// Runs the shared lane cycle workflow for a manual signal against a synthetic bar.
///
/// # Errors
///
/// Propagates any workflow error returned by the engine implementation.
pub fn run_manual_signal_cycle<E: LaneCycleEngine>(
    engine: &mut E,
    instance_id: &str,
    signal: IndicatorSignal,
    price: f64,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<E::Outcome, E::Error> {
    let phase = manual_signal_phase(signal);
    let bar = OhlcvBar {
        timestamp,
        open: price,
        high: price,
        low: price,
        close: price,
        volume: 0.0,
    };

    run_lane_cycle(
        engine,
        instance_id,
        &bar,
        phase,
        LaneCycleInvocation::ManualSignal { signal },
    )
}

fn run_lane_cycle<E: LaneCycleEngine>(
    engine: &mut E,
    instance_id: &str,
    bar: &OhlcvBar,
    phase: SignalPhase,
    invocation: LaneCycleInvocation,
) -> Result<E::Outcome, E::Error> {
    engine.ensure_kill_switch_inactive()?;
    let cycle_started_at = Instant::now();

    let result = (|| {
        let context = engine.lane_cycle_context(instance_id)?;
        if engine.has_matching_cycle_trace(
            &context.bot_id,
            &context.symbol,
            bar,
            phase,
            invocation.trigger_kind(),
            invocation.signal(),
        )? {
            return Ok(engine.duplicate_cycle_replay_outcome(
                context.bot_id,
                context.symbol,
                phase,
                context.has_position,
            ));
        }

        let trace = TraceIdentity::new(
            context.bot_id.clone(),
            context.symbol.clone(),
            bar.timestamp.to_rfc3339(),
            phase,
            invocation.trigger_kind(),
        );
        engine.sync_account_ledger(&context.account_id)?;
        let capital_before = engine.capture_cycle_capital_state(
            &context.account_id,
            &context.bot_id,
            &context.symbol,
        );
        engine.validate_account_connector_kind(
            instance_id,
            &context.account_id,
            &context.execution_connector,
        )?;
        engine.ensure_account_connector_ready(instance_id, &context.account_id)?;
        engine.ensure_connector_execution_constraints(instance_id, &context.account_id)?;
        engine.refresh_daily_loss_rollover(bar.timestamp.date_naive());

        if invocation.processes_pending_state() {
            if let Some(outcome) = engine.process_pending_warmup_bar(instance_id, bar, phase)? {
                return Ok(outcome);
            }
            if let Some(outcome) = engine.process_pending_recovery_bar(instance_id, phase)? {
                return Ok(outcome);
            }
        }

        let evaluation = match invocation {
            LaneCycleInvocation::MarketBar => {
                engine.evaluate_process_bar(instance_id, bar, phase)?
            }
            LaneCycleInvocation::ManualSignal { signal } => {
                engine.evaluate_manual_signal(instance_id, bar, phase, signal)?
            }
        };

        let mut related_events = engine.append_process_bar_records(
            instance_id,
            bar,
            phase,
            &evaluation,
            &trace.trace_id,
        )?;
        let (risk, accepted_order, execution_events) = engine.apply_risk_decision_effects(
            instance_id,
            &context.account_id,
            &context.execution_connector,
            &context.symbol,
            bar,
            &evaluation,
            &trace.trace_id,
        )?;
        related_events.extend(execution_events);
        let position_step = engine.apply_process_bar_state(
            instance_id,
            &context.account_id,
            bar,
            &evaluation,
            accepted_order.as_ref(),
            &trace.trace_id,
        )?;
        engine.persist_cycle_trace_for_evaluation(
            &trace,
            &context.account_id,
            bar,
            &evaluation,
            accepted_order.as_ref(),
            position_step,
            capital_before,
            related_events,
        )?;

        if invocation.updates_last_dispatched_bar_timestamp(phase) {
            engine.update_last_dispatched_bar_timestamp(instance_id, bar.timestamp)?;
        }

        engine.outcome_from_evaluation(instance_id, &context, phase, &evaluation, risk)
    })();

    if result.is_ok() {
        engine.record_successful_cycle_latency(cycle_started_at.elapsed());
    }

    result
}

#[derive(Debug, Clone)]
pub struct ManualCloseContext {
    pub bot_id: String,
    pub account_id: String,
    pub reconciliation_remote_snapshot: bool,
    pub has_local_position: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualCloseSignalRisk {
    Allowed,
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualCloseSignalOutcome {
    pub intent: TradeIntent,
    pub risk: ManualCloseSignalRisk,
}

#[derive(Debug, Clone)]
pub enum ManualCloseOutcome {
    AlreadyFlat,
    Processed {
        intent: TradeIntent,
        risk: ManualCloseSignalRisk,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

#[allow(clippy::missing_errors_doc)]
pub trait LaneManualOpsEngine {
    type Error;

    fn manual_close_context(&self, instance_id: &str) -> Result<ManualCloseContext, Self::Error>;
    fn sync_remote_position_for_manual_close(
        &mut self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<bool, Self::Error>;
    fn fetch_latest_bar_for_manual_close(
        &mut self,
        instance_id: &str,
    ) -> Result<OhlcvBar, Self::Error>;
    fn process_manual_close_signal(
        &mut self,
        instance_id: &str,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ManualCloseSignalOutcome, Self::Error>;
}

/// Runs the shared manual-close workflow for a lane through runtime-provided
/// connector and journal ports.
///
/// # Errors
///
/// Propagates any workflow error returned by the engine implementation.
pub fn close_lane_position<E: LaneManualOpsEngine>(
    engine: &mut E,
    instance_id: &str,
) -> Result<ManualCloseOutcome, E::Error> {
    let context = engine.manual_close_context(instance_id)?;
    let has_position = if context.reconciliation_remote_snapshot {
        engine.sync_remote_position_for_manual_close(instance_id, &context.account_id)?
    } else {
        context.has_local_position
    };

    if !has_position {
        return Ok(ManualCloseOutcome::AlreadyFlat);
    }

    let latest_bar = engine.fetch_latest_bar_for_manual_close(instance_id)?;
    let signal_outcome =
        engine.process_manual_close_signal(instance_id, latest_bar.close, latest_bar.timestamp)?;

    Ok(ManualCloseOutcome::Processed {
        intent: signal_outcome.intent,
        risk: signal_outcome.risk,
        price: latest_bar.close,
        timestamp: latest_bar.timestamp,
    })
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

#[derive(Debug)]
pub struct ConnectorSnapshotOutcome {
    pub snapshot: Option<ConnectorAccountSnapshot>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct ReconciliationSyncOutcome {
    pub position_synced: bool,
    pub stale_local_open_orders_closed_count: usize,
    pub remote_open_orders_backfilled_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStartKind {
    Started,
    Resumed,
}

impl RecoveryStartKind {
    #[must_use]
    pub fn event_kind(self) -> &'static str {
        match self {
            Self::Started => "poll.recovery.started",
            Self::Resumed => "poll.recovery.resumed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupAdvance {
    pub loaded_bars: usize,
    pub required_bars: usize,
    pub became_ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryNoProgressState {
    pub cycles: u32,
    pub should_fail: bool,
}

#[derive(Debug, Clone)]
pub struct LanePollingContext {
    pub account_id: String,
    pub data_connector: String,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub recovery_state: LaneRecoveryState,
    pub last_dispatched: Option<chrono::DateTime<chrono::Utc>>,
    pub recovery_target: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfirmedBarReplayMode {
    WarmupSeed,
    RecoveryStateOnly,
    LiveConfirmedTradable,
}

#[derive(Debug)]
pub struct LanePollingAdvance<TOutcome> {
    pub recorded_bars: Vec<OhlcvBar>,
    pub outcomes: Vec<TOutcome>,
}

impl<TOutcome> Default for LanePollingAdvance<TOutcome> {
    fn default() -> Self {
        Self {
            recorded_bars: Vec::new(),
            outcomes: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct ConfirmedBarReplayResult<TOutcome> {
    pub applied: bool,
    pub outcome: Option<TOutcome>,
}

#[derive(Debug, Clone)]
pub struct RecoveryPageApplied {
    pub account_id: String,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub recovery_target_timestamp: chrono::DateTime<chrono::Utc>,
    pub first_bar_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_bar_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub bars_applied: usize,
    pub page_limit: usize,
    pub exhausted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneWarmupContext {
    pub required_bars: usize,
    pub ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupProgressDetail {
    pub source: &'static str,
    pub loaded_bars: usize,
    pub required_bars: usize,
    pub bar_timestamp: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::missing_errors_doc, clippy::too_many_arguments)]
pub trait LanePollingEngine {
    type Error;
    type Outcome;

    fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error>;
    fn polling_context(&self, instance_id: &str) -> Result<LanePollingContext, Self::Error>;
    /// Builds an engine-specific error describing a lane invariant violation
    /// detected during polling (e.g. a `CatchingUp` lane without a recovery
    /// target). The polling loop runs where callers cannot catch a panic, so
    /// such invariant breaks are surfaced as recoverable errors instead.
    fn invariant_violation(&self, instance_id: &str, reason: &str) -> Self::Error;
    fn replay_confirmed_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        mode: ConfirmedBarReplayMode,
    ) -> Result<ConfirmedBarReplayResult<Self::Outcome>, Self::Error>;
    fn fetch_latest_bar(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, Self::Error>;
    fn fetch_latest_confirmed_bar_timestamp(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error>;
    fn fetch_confirmed_bars_range(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<chrono::DateTime<chrono::Utc>>,
        end_at: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, Self::Error>;
    fn start_lane_recovery(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        now_ms: i64,
    ) -> Result<(), Self::Error>;
    fn complete_lane_recovery(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), Self::Error>;
    fn mark_lane_out_of_sync(&mut self, instance_id: &str, reason: &str)
    -> Result<(), Self::Error>;
    fn last_dispatched_bar_timestamp(
        &self,
        instance_id: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error>;
    fn apply_recovery_page(
        &mut self,
        instance_id: &str,
        bars: &[OhlcvBar],
    ) -> Result<usize, Self::Error>;
    fn record_recovery_page_applied(
        &mut self,
        instance_id: &str,
        detail: RecoveryPageApplied,
    ) -> Result<(), Self::Error>;
    fn record_recovery_no_progress(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        exhausted: bool,
    ) -> Result<(), Self::Error>;
}

#[allow(clippy::missing_errors_doc)]
pub trait LaneWarmupEngine {
    type Error;
    type Outcome;

    fn warmup_context(&self, instance_id: &str) -> Result<LaneWarmupContext, Self::Error>;
    fn fetch_recent_bars_for_warmup(
        &mut self,
        instance_id: &str,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, Self::Error>;
    fn replay_confirmed_warmup_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
    ) -> Result<bool, Self::Error>;
    fn advance_warmup_state(
        &mut self,
        instance_id: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<WarmupAdvance>, Self::Error>;
    fn record_warmup_started(
        &mut self,
        instance_id: &str,
        source: &'static str,
        required_bars: usize,
    ) -> Result<(), Self::Error>;
    fn record_warmup_failure(
        &mut self,
        instance_id: &str,
        detail: String,
    ) -> Result<(), Self::Error>;
    fn record_warmup_progress(
        &mut self,
        instance_id: &str,
        detail: WarmupProgressDetail,
    ) -> Result<(), Self::Error>;
    fn record_warmup_ready(
        &mut self,
        instance_id: &str,
        detail: WarmupProgressDetail,
    ) -> Result<(), Self::Error>;
    fn pending_warmup_outcome(
        &self,
        instance_id: &str,
        phase: SignalPhase,
    ) -> Result<Self::Outcome, Self::Error>;
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionRecordState {
    pub has_position: bool,
    pub quantity: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessBarStateMutation {
    pub position_record: Option<PositionRecordState>,
    pub released_notional_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryTransitionFailure {
    pub action: &'static str,
    pub error: InventoryError,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct LaneRuntime {
    pub config: InstanceConfig,
    pub lane_symbol: String,
    pub execution_mode: ExecutionMode,
    pub state: LaneRuntimeState,
    pub resume_after_startup_reconcile: bool,
    pub indicators: Vec<ConfiguredIndicatorRuntime>,
    pub strategy: RuntimeStrategyEngine,
    pub bar_builder: openticker_data::BarBuilder,
    pub risk_limits: RiskLimits,
    pub target_order_notional_usd: f64,
    pub inventory: InventoryState,
    pub has_position: bool,
    pub position_quantity: f64,
    pub position_notional_usd: f64,
    pub entry_price: Option<f64>,
    pub realized_pnl_usd: f64,
    pub daily_loss_pct_accumulated: f64,
    pub last_loss_reset_date: Option<chrono::NaiveDate>,
    pub cooldown_until_ms: Option<i64>,
    pub reconciliation_blocked: bool,
    pub remote_net_qty: Option<f64>,
    pub aggregate_managed_qty: f64,
    pub external_delta_qty: Option<f64>,
    pub managed_remote_open_orders: usize,
    pub external_remote_open_orders: usize,
    pub warmup: InstanceWarmupState,
    pub recovery_state: LaneRecoveryState,
    pub recovery_started_at_ms: Option<i64>,
    pub recovery_target_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub recovery_last_progress_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub recovery_last_error: Option<String>,
    pub recovery_consecutive_no_progress_cycles: u32,
    pub last_recovered_at_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_dispatched_bar_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub last_stream_update: Option<NormalizedBarUpdate>,
    pub connector_execution_constraints: Option<ExecutionConstraintsConfig>,
    pub connector_fractional_entry_supported: Option<bool>,
    pub connector_execution_constraints_initialized: bool,
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

pub fn record_warmup_failure(state: &mut InstanceWarmupState, detail: String) {
    state.last_error = Some(detail);
}

#[must_use]
pub fn advance_warmup_state(
    state: &mut InstanceWarmupState,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Option<WarmupAdvance> {
    if state.ready {
        return None;
    }

    state.loaded_bars = state.loaded_bars.saturating_add(1);
    state.last_error = None;
    state.last_warmup_timestamp = Some(timestamp);

    let became_ready = state.loaded_bars >= state.required_bars;
    if became_ready {
        state.ready = true;
    }

    Some(WarmupAdvance {
        loaded_bars: state.loaded_bars,
        required_bars: state.required_bars,
        became_ready,
    })
}

#[must_use]
pub fn ledger_owner_path(instance: &LaneRuntime) -> LedgerOwnerPath {
    LedgerOwnerPath::new(
        instance.config.account.clone(),
        instance.config.id.clone(),
        instance.lane_symbol.clone(),
    )
}

#[must_use]
pub fn inventory_state_from_runtime_fields(
    position_quantity: f64,
    entry_price: Option<f64>,
    realized_pnl_usd: f64,
) -> InventoryState {
    InventoryState::from_position_state(position_quantity, entry_price, realized_pnl_usd)
}

pub fn sync_inventory_from_runtime_fields(instance: &mut LaneRuntime) {
    instance.inventory = inventory_state_from_runtime_fields(
        instance.position_quantity,
        instance.entry_price,
        instance.realized_pnl_usd,
    );
}

pub fn sync_runtime_fields_from_inventory(
    instance: &mut LaneRuntime,
    valuation_price: Option<f64>,
) {
    // Capture the *incoming* (pre-sync) state before any field is overwritten.
    // The genuine state-consistency anomaly this boundary guards against is a
    // lane that arrived here claiming `has_position == true` while BOTH of its
    // effective quantity sources (the ledger inventory and the cached
    // `position_quantity` field) are within tolerance of zero. That divergence
    // can be produced across the public boundary — e.g. a reconciliation
    // assessment that resolves `has_position = true` together with a ~0
    // resolved quantity (see `openticker-runtime`
    // `apply_reconciliation_assessment_state`). That assessment persists the
    // divergent fields on the lane; the anomaly is then observed here at the
    // next fill-driven sync. It is exactly the scenario that previously caused
    // `effective_position_quantity` to fabricate a quantity. Detecting it here, at the single sync boundary, lets that
    // accessor stay a clean, side-effect-free `0.0` fallback while still
    // surfacing the anomaly.
    let pre_sync_has_position = instance.has_position;
    let pre_sync_inventory_quantity = instance.inventory.quantity();
    let pre_sync_cached_quantity = instance.position_quantity;
    let inconsistent_on_entry = pre_sync_has_position
        && pre_sync_inventory_quantity <= POSITION_QUANTITY_TOLERANCE
        && pre_sync_cached_quantity <= POSITION_QUANTITY_TOLERANCE;

    instance.position_quantity = instance.inventory.quantity();
    instance.has_position = instance.position_quantity > POSITION_QUANTITY_TOLERANCE;
    instance.entry_price = instance.inventory.average_cost_usd();
    instance.realized_pnl_usd = instance.inventory.realized_pnl.net_usd;
    instance.position_notional_usd = if instance.has_position {
        instance.inventory.position_notional_usd(
            valuation_price
                .filter(|price| price.is_finite() && *price > 0.0)
                .or(instance.entry_price),
        )
    } else {
        0.0
    };

    // Record the anomaly through a release-visible channel. `recovery_last_error`
    // is the idiomatic structured lane-state marker for an operator-facing
    // anomaly: it surfaces to runtime recovery summaries (see `openticker-runtime`
    // `repo::summaries`) and is the same field `mark_lane_out_of_sync_state`
    // writes. We record it once, here, at the detection point — the read-only
    // `effective_position_quantity` accessor deliberately does NOT re-record, so
    // the inconsistency has a single coherent story. The sync above has already
    // collapsed the lane to a coherent flat state (`has_position == false`,
    // quantity/notional zeroed), so the recovered position is safe; the marker
    // exists purely so the prior divergence is visible in production, where the
    // `debug_assert!` below is compiled out.
    // Last-writer-wins overwrite is intentional: this anomaly is itself a
    // flat-reset signal, and surfacing it takes precedence over any prior
    // recovery marker for this cycle.
    if inconsistent_on_entry {
        instance.recovery_last_error = Some(format!(
            "lane position-quantity invariant violated during inventory sync: \
             account={} instance={} symbol={} had has_position=true while both \
             quantity sources were ~0 (inventory_quantity={pre_sync_inventory_quantity}, \
             cached_position_quantity={pre_sync_cached_quantity}); reset to flat",
            instance.config.account, instance.config.id, instance.lane_symbol,
        ));
    }
    // Invariant the release-visible marker above guards: a synced lane must
    // never present `has_position == true` alongside a zero effective quantity.
    // Kept as a debug-only tripwire in addition to the marker, never as the
    // only signal.
    debug_assert!(
        !(instance.has_position && instance.position_quantity <= POSITION_QUANTITY_TOLERANCE),
        "lane position-quantity invariant violated after sync: has_position={} but position_quantity={}",
        instance.has_position,
        instance.position_quantity
    );
}

#[must_use]
pub fn sync_remote_position_quantity(instance: &mut LaneRuntime, remote_quantity: f64) -> bool {
    let has_position = remote_quantity > POSITION_QUANTITY_TOLERANCE;
    let local_quantity = effective_position_quantity(instance);
    let changed = instance.has_position != has_position
        || (local_quantity - remote_quantity).abs() > POSITION_QUANTITY_TOLERANCE;
    if !changed {
        return false;
    }

    instance.has_position = has_position;
    instance.position_quantity = remote_quantity;
    if has_position {
        instance.position_notional_usd = instance.entry_price.map_or(0.0, |entry_price| {
            calculate_position_notional_usd(remote_quantity, entry_price)
        });
    } else {
        instance.position_notional_usd = 0.0;
        instance.entry_price = None;
    }
    sync_inventory_from_runtime_fields(instance);

    true
}

#[must_use]
pub fn accepted_order_fee_entry(order: &AcceptedOrder) -> Option<FeeEntry> {
    let fee_asset = order.fee_asset.clone()?;
    let fee_amount = order
        .fee_amount
        .filter(|value| value.is_finite() && *value > 0.0)?;
    Some(FeeEntry {
        asset: fee_asset,
        amount: fee_amount,
        normalized_usd: order
            .fee_normalized_usd
            .filter(|value| value.is_finite() && *value > 0.0),
    })
}

/// Returns the position quantity the lane should treat as authoritative,
/// preferring the ledger inventory, then the cached `position_quantity` field.
///
/// # Invariant
///
/// Whenever `has_position` is true the lane is expected to also carry a
/// non-zero quantity in either the inventory or the `position_quantity`
/// field (see `sync_runtime_fields_from_inventory`, where the two are kept in
/// lockstep). If that invariant is ever violated — `has_position == true`
/// while both quantity sources are within `POSITION_QUANTITY_TOLERANCE` of
/// zero — this function deliberately returns `0.0` rather than fabricating a
/// quantity. Returning a fabricated non-zero value here previously corrupted
/// downstream position-notional and order-sizing math, so `0.0` is the only
/// safe answer: it makes notional zero and prevents fabricated sizing. The
/// inconsistency itself is detected and recorded at the state-sync boundary
/// (`sync_runtime_fields_from_inventory`, via the release-visible
/// `recovery_last_error` marker plus a debug-only tripwire), not here, so that
/// this read-only accessor stays a side-effect-free, panic-free `0.0` fallback
/// that never double-records the anomaly.
#[must_use]
pub fn effective_position_quantity(instance: &LaneRuntime) -> f64 {
    if instance.inventory.quantity() > POSITION_QUANTITY_TOLERANCE {
        instance.inventory.quantity()
    } else if instance.position_quantity > POSITION_QUANTITY_TOLERANCE {
        instance.position_quantity
    } else {
        // Defensive: `has_position` may be true here only if the
        // quantity-consistency invariant was violated upstream. Never
        // fabricate a quantity; return zero so notional collapses to zero.
        0.0
    }
}

#[must_use]
pub fn current_instance_open_notional_usd(instance: &LaneRuntime) -> f64 {
    sanitize_ledger_value(instance.position_notional_usd)
}

#[must_use]
pub fn aggregate_bot_state(lanes: &[&LaneRuntime]) -> LaneRuntimeState {
    if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Reconciling))
    {
        LaneRuntimeState::Reconciling
    } else if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Running))
    {
        LaneRuntimeState::Running
    } else if lanes
        .iter()
        .any(|lane| matches!(lane.state, LaneRuntimeState::Paused))
    {
        LaneRuntimeState::Paused
    } else {
        LaneRuntimeState::Stopped
    }
}

pub fn mark_recovery_page_progress(
    instance: &mut LaneRuntime,
    timestamp: chrono::DateTime<chrono::Utc>,
) {
    instance.recovery_last_progress_timestamp = Some(timestamp);
    instance.last_recovered_at_timestamp = Some(timestamp);
    instance.recovery_consecutive_no_progress_cycles = 0;
    instance.recovery_last_error = None;
}

#[must_use]
pub fn start_lane_recovery_state(
    instance: &mut LaneRuntime,
    target: chrono::DateTime<chrono::Utc>,
    now_ms: i64,
) -> RecoveryStartKind {
    let kind = if instance.recovery_state == LaneRecoveryState::OutOfSync {
        RecoveryStartKind::Resumed
    } else {
        RecoveryStartKind::Started
    };

    instance.recovery_state = LaneRecoveryState::CatchingUp;
    instance.recovery_started_at_ms = Some(now_ms);
    instance.recovery_target_timestamp = Some(target);
    instance.recovery_last_error = None;
    instance.recovery_consecutive_no_progress_cycles = 0;

    kind
}

#[must_use]
pub fn complete_lane_recovery_state(
    instance: &mut LaneRuntime,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let last_progress_timestamp = instance.recovery_last_progress_timestamp;
    instance.recovery_state = LaneRecoveryState::Healthy;
    instance.recovery_target_timestamp = None;
    instance.recovery_last_error = None;
    instance.recovery_consecutive_no_progress_cycles = 0;
    last_progress_timestamp
}

pub fn mark_lane_out_of_sync_state(instance: &mut LaneRuntime, reason: &str) {
    instance.recovery_state = LaneRecoveryState::OutOfSync;
    instance.recovery_last_error = Some(reason.to_owned());
    instance.recovery_target_timestamp = instance
        .recovery_target_timestamp
        .or(instance.last_dispatched_bar_timestamp);
}

#[must_use]
pub fn record_recovery_no_progress_state(
    instance: &mut LaneRuntime,
    exhausted: bool,
    max_recovery_no_progress_cycles: u32,
) -> RecoveryNoProgressState {
    instance.recovery_consecutive_no_progress_cycles = instance
        .recovery_consecutive_no_progress_cycles
        .saturating_add(1);

    RecoveryNoProgressState {
        cycles: instance.recovery_consecutive_no_progress_cycles,
        should_fail: exhausted
            || instance.recovery_consecutive_no_progress_cycles >= max_recovery_no_progress_cycles,
    }
}

/// Validates that recovery bars are strictly increasing and stay within the
/// requested recovery window.
///
/// # Errors
///
/// Returns an error if the bars are not strictly increasing or if a bar
/// timestamp exceeds `end_at`.
pub fn validate_recovery_bars(
    start_after: Option<chrono::DateTime<chrono::Utc>>,
    end_at: chrono::DateTime<chrono::Utc>,
    bars: &[OhlcvBar],
) -> Result<(), String> {
    let mut previous = start_after;
    for bar in bars {
        if previous.is_some_and(|timestamp| bar.timestamp <= timestamp) {
            return Err(format!(
                "connector recovery bars are not strictly increasing around {}",
                bar.timestamp.to_rfc3339()
            ));
        }
        if bar.timestamp > end_at {
            return Err(format!(
                "connector recovery bar {} exceeded target {}",
                bar.timestamp.to_rfc3339(),
                end_at.to_rfc3339()
            ));
        }
        previous = Some(bar.timestamp);
    }

    Ok(())
}

/// Advances a lane through one polling cycle, including recovery catch-up when needed.
///
/// # Errors
///
/// Propagates any engine error produced while fetching connector data or
/// applying lane state. Also returns an engine error (via
/// [`LanePollingEngine::invariant_violation`]) if the polling context reports
/// `CatchingUp` without a recovery target: that indicates an internal
/// invariant violation, and because this runs inside the polling loop where
/// callers cannot catch a panic, it is surfaced as a recoverable error rather
/// than aborting the whole loop.
///
/// # Panics
///
/// Panics only if the `last_dispatched` timestamp becomes `None` after it has
/// already been confirmed `Some` earlier in the same call. That is impossible
/// because `context` is read once and never mutated, so the `expect` is purely
/// a guard against future refactors.
#[allow(clippy::too_many_lines)]
pub fn advance_lane_polling_once<E: LanePollingEngine>(
    engine: &mut E,
    instance_id: &str,
    page_limit: usize,
    max_pages_per_cycle: usize,
    now_ms: i64,
) -> Result<LanePollingAdvance<E::Outcome>, E::Error>
where
    E::Error: std::fmt::Display,
{
    engine.ensure_kill_switch_inactive()?;
    let context = engine.polling_context(instance_id)?;

    if context.recovery_state == LaneRecoveryState::CatchingUp {
        let Some(target) = context.recovery_target else {
            return Err(engine.invariant_violation(
                instance_id,
                "polling context reported CatchingUp without a recovery target",
            ));
        };
        return run_recovery_pages(
            engine,
            instance_id,
            &context.account_id,
            &context.data_connector,
            &context.symbol,
            context.timeframe,
            target,
            page_limit,
            max_pages_per_cycle,
            None,
        );
    }

    if context.last_dispatched.is_none() {
        let latest_bar = engine.fetch_latest_bar(
            instance_id,
            &context.account_id,
            &context.data_connector,
            &context.symbol,
            context.timeframe,
        )?;
        let replay = engine.replay_confirmed_bar(
            instance_id,
            &latest_bar,
            ConfirmedBarReplayMode::LiveConfirmedTradable,
        )?;
        let mut advance = LanePollingAdvance::default();
        if replay.applied {
            advance.recorded_bars.push(latest_bar);
        }
        if let Some(outcome) = replay.outcome {
            advance.outcomes.push(outcome);
        }
        return Ok(advance);
    }

    let latest_confirmed = engine.fetch_latest_confirmed_bar_timestamp(
        instance_id,
        &context.account_id,
        &context.data_connector,
        &context.symbol,
        context.timeframe,
    )?;
    let Some(latest_target) = latest_confirmed else {
        return Ok(LanePollingAdvance::default());
    };
    let last_dispatched = context.last_dispatched.expect("checked above");
    if latest_target <= last_dispatched {
        let latest_bar = engine.fetch_latest_bar(
            instance_id,
            &context.account_id,
            &context.data_connector,
            &context.symbol,
            context.timeframe,
        )?;
        if context.recovery_state != LaneRecoveryState::Healthy {
            engine.complete_lane_recovery(instance_id, "target already current")?;
        }
        let mut advance = LanePollingAdvance::default();
        if latest_bar.timestamp == last_dispatched {
            advance.recorded_bars.push(latest_bar);
        }
        return Ok(advance);
    }

    let first_page = match engine.fetch_confirmed_bars_range(
        instance_id,
        &context.account_id,
        &context.data_connector,
        &context.symbol,
        context.timeframe,
        Some(last_dispatched),
        latest_target,
        page_limit.max(2),
    ) {
        Ok(page) => page,
        Err(error) => {
            engine.mark_lane_out_of_sync(
                instance_id,
                &format!("recovery target fetch failed: {error}"),
            )?;
            return Err(error);
        }
    };

    if let Err(reason) =
        validate_recovery_bars(Some(last_dispatched), latest_target, &first_page.bars)
    {
        engine.mark_lane_out_of_sync(instance_id, &reason)?;
        return Ok(LanePollingAdvance::default());
    }

    if context.recovery_state == LaneRecoveryState::Healthy
        && first_page.exhausted
        && first_page.bars.len() == 1
        && first_page.bars[0].timestamp == latest_target
    {
        let latest_bar = first_page.bars[0].clone();
        let replay = engine.replay_confirmed_bar(
            instance_id,
            &latest_bar,
            ConfirmedBarReplayMode::LiveConfirmedTradable,
        )?;
        let mut advance = LanePollingAdvance::default();
        if replay.applied {
            advance.recorded_bars.push(latest_bar);
        }
        if let Some(outcome) = replay.outcome {
            advance.outcomes.push(outcome);
        }
        return Ok(advance);
    }

    engine.start_lane_recovery(instance_id, latest_target, now_ms)?;
    run_recovery_pages(
        engine,
        instance_id,
        &context.account_id,
        &context.data_connector,
        &context.symbol,
        context.timeframe,
        latest_target,
        page_limit,
        max_pages_per_cycle,
        Some(first_page),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_recovery_pages<E: LanePollingEngine>(
    engine: &mut E,
    instance_id: &str,
    account_id: &str,
    data_connector: &str,
    symbol: &str,
    timeframe: Timeframe,
    target: chrono::DateTime<chrono::Utc>,
    page_limit: usize,
    max_pages_per_cycle: usize,
    first_page: Option<ConfirmedBarPage>,
) -> Result<LanePollingAdvance<E::Outcome>, E::Error>
where
    E::Error: std::fmt::Display,
{
    let mut advance = LanePollingAdvance::default();
    let mut page = first_page;

    for _ in 0..max_pages_per_cycle.max(1) {
        let current_last_dispatched = engine.last_dispatched_bar_timestamp(instance_id)?;
        let Some(start_after) = current_last_dispatched else {
            engine.mark_lane_out_of_sync(
                instance_id,
                "recovery lost the authoritative last dispatched timestamp",
            )?;
            return Ok(advance);
        };

        let current_page = match page.take() {
            Some(page) => page,
            None => match engine.fetch_confirmed_bars_range(
                instance_id,
                account_id,
                data_connector,
                symbol,
                timeframe,
                Some(start_after),
                target,
                page_limit.max(1),
            ) {
                Ok(page) => page,
                Err(error) => {
                    engine.mark_lane_out_of_sync(
                        instance_id,
                        &format!("recovery page fetch failed: {error}"),
                    )?;
                    return Err(error);
                }
            },
        };

        if let Err(reason) = validate_recovery_bars(Some(start_after), target, &current_page.bars) {
            engine.mark_lane_out_of_sync(instance_id, &reason)?;
            return Ok(advance);
        }

        if current_page.bars.is_empty() {
            engine.record_recovery_no_progress(instance_id, target, current_page.exhausted)?;
            return Ok(advance);
        }

        let first_timestamp = current_page.bars.first().map(|bar| bar.timestamp);
        let last_timestamp = current_page.bars.last().map(|bar| bar.timestamp);
        let bars_applied = engine.apply_recovery_page(instance_id, &current_page.bars)?;
        advance.recorded_bars.extend(current_page.bars.clone());
        engine.record_recovery_page_applied(
            instance_id,
            RecoveryPageApplied {
                account_id: account_id.to_owned(),
                symbol: symbol.to_owned(),
                timeframe,
                recovery_target_timestamp: target,
                first_bar_timestamp: first_timestamp,
                last_bar_timestamp: last_timestamp,
                bars_applied,
                page_limit,
                exhausted: current_page.exhausted,
            },
        )?;

        let updated_last_dispatched = engine.last_dispatched_bar_timestamp(instance_id)?;
        if updated_last_dispatched.is_some_and(|timestamp| timestamp >= target) {
            engine.complete_lane_recovery(instance_id, "recovery target reached")?;
            return Ok(advance);
        }

        if current_page.exhausted {
            engine.mark_lane_out_of_sync(
                instance_id,
                &format!(
                    "confirmed history exhausted before reaching recovery target {}",
                    target.to_rfc3339()
                ),
            )?;
            return Ok(advance);
        }
    }

    Ok(advance)
}

/// Attempts to backfill warmup history for a lane through the provided engine.
///
/// # Errors
///
/// Propagates engine errors from warmup state access or event persistence.
pub fn attempt_lane_warmup_backfill<E: LaneWarmupEngine>(
    engine: &mut E,
    instance_id: &str,
    source: &'static str,
) -> Result<(), E::Error>
where
    E::Error: std::fmt::Display,
{
    let context = engine.warmup_context(instance_id)?;
    if context.ready || context.required_bars == 0 {
        return Ok(());
    }

    engine.record_warmup_started(instance_id, source, context.required_bars)?;

    match engine.fetch_recent_bars_for_warmup(instance_id, context.required_bars) {
        Ok(bars) => {
            if bars.is_empty() {
                engine.record_warmup_failure(
                    instance_id,
                    format!("{source} warmup backfill returned no confirmed bars"),
                )?;
            } else {
                for bar in &bars {
                    let applied = apply_confirmed_warmup_bar(engine, instance_id, bar, source)?;
                    if applied && engine.warmup_context(instance_id)?.ready {
                        break;
                    }
                }
            }
            Ok(())
        }
        Err(error) => {
            engine.record_warmup_failure(
                instance_id,
                format!("{source} warmup backfill unavailable: {error}"),
            )?;
            Ok(())
        }
    }
}

/// Handles a bar while warmup is still pending for the lane.
///
/// # Errors
///
/// Propagates engine errors from replay, warmup state mutation, or outcome construction.
pub fn process_pending_warmup_bar<E: LaneWarmupEngine>(
    engine: &mut E,
    instance_id: &str,
    bar: &OhlcvBar,
    phase: SignalPhase,
) -> Result<Option<E::Outcome>, E::Error> {
    if engine.warmup_context(instance_id)?.ready {
        return Ok(None);
    }

    if matches!(phase, SignalPhase::Confirmed) {
        let _ = apply_confirmed_warmup_bar(engine, instance_id, bar, "live_confirmed")?;
    }

    engine.pending_warmup_outcome(instance_id, phase).map(Some)
}

fn apply_confirmed_warmup_bar<E: LaneWarmupEngine>(
    engine: &mut E,
    instance_id: &str,
    bar: &OhlcvBar,
    source: &'static str,
) -> Result<bool, E::Error> {
    if !engine.replay_confirmed_warmup_bar(instance_id, bar)? {
        return Ok(false);
    }

    let Some(progress) = engine.advance_warmup_state(instance_id, bar.timestamp)? else {
        return Ok(false);
    };

    let detail = WarmupProgressDetail {
        source,
        loaded_bars: progress.loaded_bars,
        required_bars: progress.required_bars,
        bar_timestamp: bar.timestamp,
    };
    engine.record_warmup_progress(instance_id, detail)?;
    if progress.became_ready {
        engine.record_warmup_ready(instance_id, detail)?;
    }

    Ok(true)
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

fn signal_phase_label(phase: SignalPhase) -> &'static str {
    match phase {
        SignalPhase::Preview => "preview",
        SignalPhase::Confirmed => "confirmed",
    }
}

fn indicator_signal_label(signal: IndicatorSignal) -> &'static str {
    match signal {
        IndicatorSignal::None => "none",
        IndicatorSignal::BuyPreview => "buy_preview",
        IndicatorSignal::BuyConfirmed => "buy_confirmed",
        IndicatorSignal::SellPreview => "sell_preview",
        IndicatorSignal::SellConfirmed => "sell_confirmed",
    }
}

fn strategy_signal_source_label(source: StrategySignalSource) -> &'static str {
    match source {
        StrategySignalSource::Representative => "representative",
        StrategySignalSource::IntentFallback => "intent_fallback",
        StrategySignalSource::Manual => "manual",
    }
}

fn trade_intent_label(intent: TradeIntent) -> &'static str {
    match intent {
        TradeIntent::NoOp => "no_op",
        TradeIntent::OpenLong => "open_long",
        TradeIntent::AddLong => "add_long",
        TradeIntent::ReduceLong => "reduce_long",
        TradeIntent::CloseLong => "close_long",
    }
}

fn ledger_outcome_reason_code(outcome: OrderLedgerOutcome) -> &'static str {
    match outcome {
        OrderLedgerOutcome::BotExhausted => "bot_ledger_exhausted",
        OrderLedgerOutcome::AccountExhausted => "account_ledger_exhausted",
        OrderLedgerOutcome::Dust => "ledger_dust",
    }
}

fn trace_event(scope: &str, kind: &str, payload: Value) -> RelatedEvent {
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

fn position_record_state(instance: &LaneRuntime) -> PositionRecordState {
    PositionRecordState {
        has_position: instance.has_position,
        quantity: effective_position_quantity(instance),
        entry_price: instance.entry_price,
        realized_pnl_usd: instance.realized_pnl_usd,
    }
}

fn apply_open_long_fill(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    order: &AcceptedOrder,
) -> Result<(), InventoryTransitionFailure> {
    let fill_quantity = order.quantity.max(0.0);
    if fill_quantity <= f64::EPSILON {
        return Ok(());
    }

    let previous_quantity = effective_position_quantity(instance);
    let fee_entry = accepted_order_fee_entry(order);
    sync_inventory_from_runtime_fields(instance);
    let average_cost = instance.inventory.average_cost_usd();

    if average_cost.is_some() || previous_quantity <= POSITION_QUANTITY_TOLERANCE {
        instance
            .inventory
            .apply_fill(
                InventoryFillSide::Buy,
                fill_quantity,
                order.price,
                fee_entry.as_ref(),
            )
            .map_err(|error| InventoryTransitionFailure {
                action: "open_fill",
                error,
            })?;
        sync_runtime_fields_from_inventory(instance, Some(bar.close));
    } else {
        let previous_entry_price = instance.entry_price.unwrap_or(order.price);
        let new_quantity = previous_quantity + fill_quantity;
        let new_entry_price = ((previous_quantity * previous_entry_price)
            + (fill_quantity * order.price))
            / new_quantity;

        instance.position_quantity = new_quantity;
        instance.position_notional_usd = calculate_position_notional_usd(new_quantity, bar.close);
        instance.entry_price = Some(new_entry_price);
        instance.has_position = new_quantity > f64::EPSILON;
        sync_inventory_from_runtime_fields(instance);
    }

    Ok(())
}

fn apply_close_long_fill(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    order: &AcceptedOrder,
) -> Result<f64, InventoryTransitionFailure> {
    let previous_quantity = effective_position_quantity(instance);
    let fee_entry = accepted_order_fee_entry(order);
    sync_inventory_from_runtime_fields(instance);
    let fill_quantity = order.quantity.max(0.0).min(previous_quantity);
    let released_notional_usd = calculate_position_notional_usd(fill_quantity, bar.close);
    let average_cost = instance.inventory.average_cost_usd();

    if let Some(entry_price) = average_cost
        && entry_price > 0.0
        && previous_quantity > f64::EPSILON
        && fill_quantity > f64::EPSILON
    {
        let pnl_pct = ((order.price - entry_price) / entry_price) * 100.0;
        if pnl_pct < 0.0 {
            let closed_fraction = (fill_quantity / previous_quantity).clamp(0.0, 1.0);
            instance.daily_loss_pct_accumulated += -pnl_pct * closed_fraction;
        }
    }

    if fill_quantity > f64::EPSILON {
        if average_cost.is_some() {
            instance
                .inventory
                .apply_fill(
                    InventoryFillSide::Sell,
                    fill_quantity,
                    order.price,
                    fee_entry.as_ref(),
                )
                .map_err(|error| InventoryTransitionFailure {
                    action: "close_fill",
                    error,
                })?;
            sync_runtime_fields_from_inventory(instance, Some(bar.close));
        } else {
            let remaining_quantity = (previous_quantity - fill_quantity).max(0.0);
            instance.position_quantity = remaining_quantity;
            instance.position_notional_usd =
                calculate_position_notional_usd(remaining_quantity, bar.close);
            instance.has_position = remaining_quantity > f64::EPSILON;
            if !instance.has_position {
                instance.entry_price = None;
            }
            sync_inventory_from_runtime_fields(instance);
        }
    }

    Ok(released_notional_usd)
}

/// Applies the fill-state transition for a lane after risk has resolved the
/// trade intent.
///
/// # Errors
///
/// Returns an inventory transition failure when the lane's local inventory
/// cannot absorb the accepted buy or sell fill.
pub fn apply_process_bar_fill_state(
    instance: &mut LaneRuntime,
    bar: &OhlcvBar,
    next_has_position: bool,
    accepted_order: Option<&AcceptedOrder>,
    order_ledger_outcome: Option<OrderLedgerOutcome>,
    risk_decision: &RiskDecision,
) -> Result<ProcessBarStateMutation, InventoryTransitionFailure> {
    let mut position_record = None;
    let mut released_notional_usd = None;

    match risk_decision {
        RiskDecision::Allow(allowed_intent) => match allowed_intent {
            TradeIntent::OpenLong | TradeIntent::AddLong => {
                if let Some(order) = accepted_order {
                    apply_open_long_fill(instance, bar, order)?;
                    position_record = Some(position_record_state(instance));
                }
                instance.cooldown_until_ms = None;
            }
            TradeIntent::CloseLong | TradeIntent::ReduceLong => {
                if let Some(order) = accepted_order {
                    released_notional_usd = Some(apply_close_long_fill(instance, bar, order)?);
                    position_record = Some(position_record_state(instance));
                }
                instance.cooldown_until_ms = None;
            }
            TradeIntent::NoOp => {
                instance.has_position = next_has_position;
            }
        },
        RiskDecision::Reject { .. } => {
            instance.has_position = next_has_position;
            if order_ledger_outcome.is_none() {
                let cooldown_ms = i64::try_from(instance.risk_limits.cooldown_after_reject_ms)
                    .unwrap_or(i64::MAX);
                instance.cooldown_until_ms =
                    Some(bar.timestamp.timestamp_millis().saturating_add(cooldown_ms));
            }
        }
    }

    Ok(ProcessBarStateMutation {
        position_record,
        released_notional_usd,
    })
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

#[cfg(test)]
mod tests {
    use super::{
        ConfirmedBarPage, ConfirmedBarReplayMode, ConfirmedBarReplayResult, InstanceWarmupState,
        LaneManualOpsEngine, LanePollingContext, LanePollingEngine, LaneRecoveryState,
        LaneRuntimeState, ManualCloseContext, ManualCloseOutcome, ManualCloseSignalOutcome,
        ManualCloseSignalRisk, RecoveryPageApplied, RecoveryStartKind, accepted_order_fee_entry,
        advance_lane_polling_once, advance_warmup_state, apply_process_bar_fill_state,
        close_lane_position, complete_lane_recovery_state, effective_position_quantity,
        mark_lane_out_of_sync_state, record_recovery_no_progress_state, record_warmup_failure,
        start_lane_recovery_state, sync_remote_position_quantity,
        sync_runtime_fields_from_inventory, validate_recovery_bars,
    };
    use openticker_config::{
        BudgetConfig, ExecutionConstraintsConfig, InstanceConfig, InstanceRiskConfig,
        RiskOverrides, SignalMode,
    };
    use openticker_core::{ExecutionMode, MarketType, OhlcvBar, Timeframe, TradeIntent};
    use openticker_execution::{AcceptedOrder, OrderSide, OrderType};
    use openticker_instance::build_runtime_strategy;
    use openticker_ledger::InventoryState;
    use openticker_risk::{RiskDecision, RiskLimits};

    const FLOAT_ASSERT_EPSILON: f64 = 1e-9;

    fn assert_f64_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < FLOAT_ASSERT_EPSILON,
            "expected {actual} to be within {FLOAT_ASSERT_EPSILON} of {expected}"
        );
    }

    fn assert_opt_f64_close(actual: Option<f64>, expected: f64) {
        assert!(
            actual.is_some_and(|value| (value - expected).abs() < FLOAT_ASSERT_EPSILON),
            "expected {actual:?} to contain a value within {FLOAT_ASSERT_EPSILON} of {expected}"
        );
    }

    #[test]
    fn lane_runtime_state_round_trips_storage_values() {
        assert_eq!(
            LaneRuntimeState::from_storage_value(LaneRuntimeState::Running.as_storage_value()),
            Some(LaneRuntimeState::Running)
        );
        assert_eq!(LaneRuntimeState::from_storage_value("unknown"), None);
    }

    #[test]
    fn warmup_state_is_ready_only_when_no_bars_are_required() {
        assert!(InstanceWarmupState::new(0).ready);
        assert!(!InstanceWarmupState::new(10).ready);
    }

    #[test]
    fn accepted_order_fee_entry_filters_invalid_fee_values() {
        let valid = AcceptedOrder {
            client_order_id: "order-1".to_owned(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            price: 100.0,
            quantity: 1.0,
            fee_asset: Some("USD".to_owned()),
            fee_amount: Some(0.5),
            fee_normalized_usd: Some(0.5),
        };
        let invalid = AcceptedOrder {
            fee_amount: Some(0.0),
            ..valid.clone()
        };

        let fee = accepted_order_fee_entry(&valid).expect("fee entry should exist");
        assert_eq!(fee.asset, "USD");
        assert_f64_close(fee.amount, 0.5);
        assert_opt_f64_close(fee.normalized_usd, 0.5);
        assert!(accepted_order_fee_entry(&invalid).is_none());
    }

    #[test]
    fn warmup_helpers_record_failure_and_ready_transition() {
        let mut warmup = InstanceWarmupState::new(2);
        record_warmup_failure(&mut warmup, "fetch failed".to_owned());
        assert_eq!(warmup.last_error.as_deref(), Some("fetch failed"));

        let timestamp = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let first = advance_warmup_state(&mut warmup, timestamp).expect("advance should apply");
        assert_eq!(first.loaded_bars, 1);
        assert!(!first.became_ready);
        let second = advance_warmup_state(&mut warmup, timestamp).expect("advance should apply");
        assert_eq!(second.loaded_bars, 2);
        assert!(second.became_ready);
        assert!(warmup.ready);
        assert!(advance_warmup_state(&mut warmup, timestamp).is_none());
    }

    #[test]
    fn recovery_helpers_track_start_completion_and_failure() {
        let mut lane = test_lane_runtime();
        lane.last_dispatched_bar_timestamp = Some(
            chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&chrono::Utc),
        );
        let target = chrono::DateTime::parse_from_rfc3339("2030-01-01T01:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);

        let kind = start_lane_recovery_state(&mut lane, target, 123);
        assert_eq!(kind, RecoveryStartKind::Started);
        assert_eq!(lane.recovery_state, LaneRecoveryState::CatchingUp);
        assert_eq!(lane.recovery_started_at_ms, Some(123));

        let no_progress = record_recovery_no_progress_state(&mut lane, false, 3);
        assert_eq!(no_progress.cycles, 1);
        assert!(!no_progress.should_fail);

        mark_lane_out_of_sync_state(&mut lane, "stalled");
        assert_eq!(lane.recovery_state, LaneRecoveryState::OutOfSync);
        assert_eq!(lane.recovery_last_error.as_deref(), Some("stalled"));

        let resumed = start_lane_recovery_state(&mut lane, target, 456);
        assert_eq!(resumed, RecoveryStartKind::Resumed);
        lane.recovery_last_progress_timestamp = Some(target);
        let completed = complete_lane_recovery_state(&mut lane);
        assert_eq!(completed, Some(target));
        assert_eq!(lane.recovery_state, LaneRecoveryState::Healthy);
        assert!(lane.recovery_target_timestamp.is_none());
    }

    #[test]
    fn process_bar_fill_state_updates_open_position_and_position_record() {
        let mut lane = test_lane_runtime();
        let bar = test_bar(101.0);
        let accepted_order = AcceptedOrder {
            client_order_id: "order-1".to_owned(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            price: 100.0,
            quantity: 2.0,
            fee_asset: Some("USD".to_owned()),
            fee_amount: Some(0.5),
            fee_normalized_usd: Some(0.5),
        };

        let mutation = apply_process_bar_fill_state(
            &mut lane,
            &bar,
            true,
            Some(&accepted_order),
            None,
            &RiskDecision::Allow(TradeIntent::OpenLong),
        )
        .expect("fill application should succeed");

        assert!(mutation.released_notional_usd.is_none());
        let position_record = mutation
            .position_record
            .expect("position record should exist");
        assert!(position_record.has_position);
        assert!((position_record.quantity - 2.0).abs() < 1e-9);
        assert!(lane.has_position);
        assert!((lane.position_quantity - 2.0).abs() < 1e-9);
        assert!(lane.entry_price.is_some());
    }

    #[test]
    fn process_bar_fill_state_releases_closed_notional_and_tracks_loss() {
        let mut lane = test_lane_runtime();
        lane.has_position = true;
        lane.position_quantity = 2.0;
        lane.entry_price = Some(100.0);
        lane.position_notional_usd = 200.0;
        lane.inventory = InventoryState::from_position_state(2.0, Some(100.0), 0.0);
        let bar = test_bar(90.0);
        let accepted_order = AcceptedOrder {
            client_order_id: "order-2".to_owned(),
            side: OrderSide::Sell,
            order_type: OrderType::Market,
            price: 90.0,
            quantity: 1.0,
            fee_asset: None,
            fee_amount: None,
            fee_normalized_usd: None,
        };

        let mutation = apply_process_bar_fill_state(
            &mut lane,
            &bar,
            false,
            Some(&accepted_order),
            None,
            &RiskDecision::Allow(TradeIntent::ReduceLong),
        )
        .expect("fill application should succeed");

        assert_opt_f64_close(mutation.released_notional_usd, 90.0);
        assert!(lane.daily_loss_pct_accumulated > 0.0);
        let position_record = mutation
            .position_record
            .expect("position record should exist");
        assert!(position_record.has_position);
        assert!((position_record.quantity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rejected_process_bar_sets_reject_cooldown_without_inventory_mutation() {
        let mut lane = test_lane_runtime();
        let bar = test_bar(101.0);

        let mutation = apply_process_bar_fill_state(
            &mut lane,
            &bar,
            false,
            None,
            None,
            &RiskDecision::Reject {
                reason: "risk_limit",
            },
        )
        .expect("reject application should succeed");

        assert!(mutation.position_record.is_none());
        assert!(mutation.released_notional_usd.is_none());
        assert!(lane.cooldown_until_ms.is_some());
    }

    #[test]
    fn remote_position_sync_updates_runtime_fields() {
        let mut lane = test_lane_runtime();
        lane.entry_price = Some(100.0);

        assert!(sync_remote_position_quantity(&mut lane, 3.0));
        assert!(lane.has_position);
        assert!((lane.position_quantity - 3.0).abs() < 1e-9);
        assert!((lane.position_notional_usd - 300.0).abs() < 1e-9);

        assert!(sync_remote_position_quantity(&mut lane, 0.0));
        assert!(!lane.has_position);
        assert_eq!(lane.entry_price, None);
        assert_f64_close(lane.position_notional_usd, 0.0);
    }

    #[test]
    fn recovery_bar_validation_rejects_out_of_order_and_future_bars() {
        let start_after = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);
        let end_at = chrono::DateTime::parse_from_rfc3339("2030-01-01T00:02:00Z")
            .expect("timestamp should parse")
            .with_timezone(&chrono::Utc);

        let out_of_order = vec![test_bar_at(101.0, 1), test_bar_at(102.0, 0)];
        assert!(validate_recovery_bars(Some(start_after), end_at, &out_of_order).is_err());

        let future_bar = vec![test_bar_at(101.0, 3)];
        assert!(validate_recovery_bars(Some(start_after), end_at, &future_bar).is_err());
    }

    #[test]
    fn manual_close_skips_when_lane_is_already_flat() {
        let mut engine = MockManualCloseEngine {
            context: ManualCloseContext {
                bot_id: "bot-a".to_owned(),
                account_id: "acct".to_owned(),
                reconciliation_remote_snapshot: false,
                has_local_position: false,
            },
            ..MockManualCloseEngine::default()
        };

        let outcome =
            close_lane_position(&mut engine, "bot-a").expect("manual close should succeed");

        assert!(matches!(outcome, ManualCloseOutcome::AlreadyFlat));
        assert!(!engine.fetch_latest_bar_called);
        assert!(!engine.process_manual_close_signal_called);
    }

    #[test]
    fn manual_close_uses_remote_sync_before_submitting_signal() {
        let mut engine = MockManualCloseEngine {
            context: ManualCloseContext {
                bot_id: "bot-a".to_owned(),
                account_id: "acct".to_owned(),
                reconciliation_remote_snapshot: true,
                has_local_position: false,
            },
            remote_has_position: true,
            latest_bar: test_bar(101.0),
            signal_outcome: ManualCloseSignalOutcome {
                intent: TradeIntent::CloseLong,
                risk: ManualCloseSignalRisk::Allowed,
            },
            ..MockManualCloseEngine::default()
        };

        let outcome =
            close_lane_position(&mut engine, "bot-a").expect("manual close should succeed");

        assert!(engine.sync_remote_position_called);
        assert!(matches!(
            outcome,
            ManualCloseOutcome::Processed {
                intent: TradeIntent::CloseLong,
                risk: ManualCloseSignalRisk::Allowed,
                price,
                ..
            } if (price - 101.0).abs() < 1e-9
        ));
    }

    #[test]
    fn effective_position_quantity_never_fabricates_when_quantity_is_zero() {
        let mut lane = test_lane_runtime();
        // Inconsistent state: the lane claims a position while both quantity
        // sources are zero. This previously returned a fabricated `1.0`, which
        // corrupted notional and order-sizing math downstream.
        lane.has_position = true;
        lane.position_quantity = 0.0;
        lane.inventory = InventoryState::default();

        assert_f64_close(effective_position_quantity(&lane), 0.0);

        // Sanity: a genuine position still reports its real quantity.
        lane.position_quantity = 2.5;
        assert_f64_close(effective_position_quantity(&lane), 2.5);
    }

    #[test]
    fn sync_runtime_fields_records_inconsistency_via_recovery_last_error() {
        let mut lane = test_lane_runtime();
        // Construct the genuinely inconsistent *pre-sync* state that can arise
        // across the public boundary (e.g. a reconciliation assessment that
        // resolves `has_position = true` with a ~0 resolved quantity): the lane
        // claims a position while BOTH effective quantity sources are ~0.
        lane.has_position = true;
        lane.position_quantity = 0.0;
        lane.inventory = InventoryState::default();
        lane.recovery_last_error = None;

        sync_runtime_fields_from_inventory(&mut lane, Some(100.0));

        // The sync collapses the lane to a coherent flat state ...
        assert!(!lane.has_position);
        assert_f64_close(lane.position_quantity, 0.0);
        assert_f64_close(lane.position_notional_usd, 0.0);
        // ... and the read-only accessor still refuses to fabricate a quantity.
        assert_f64_close(effective_position_quantity(&lane), 0.0);
        // ... while the prior divergence is recorded on a release-visible
        // channel so an operator can see it in production.
        let recorded = lane
            .recovery_last_error
            .as_deref()
            .expect("inconsistency should be recorded via recovery_last_error");
        assert!(
            recorded.contains("position-quantity invariant violated"),
            "unexpected recovery_last_error: {recorded}"
        );
        assert!(
            recorded.contains("symbol=AAPL") && recorded.contains("instance=bot-a"),
            "recovery_last_error should carry debug context: {recorded}"
        );
    }

    #[test]
    fn sync_runtime_fields_leaves_recovery_last_error_clear_when_consistent() {
        let mut lane = test_lane_runtime();
        // A genuine flat lane (no claimed position, zero quantity) is NOT an
        // inconsistency and must not be flagged.
        lane.recovery_last_error = None;
        sync_runtime_fields_from_inventory(&mut lane, Some(100.0));
        assert!(
            lane.recovery_last_error.is_none(),
            "a consistent flat lane must not flag an invariant violation"
        );

        // A lane that closes out normally (cached quantity still non-zero at
        // entry while inventory has zeroed) is the expected close transition,
        // not the both-sources-zero anomaly, so it must not be flagged either.
        lane.has_position = true;
        lane.position_quantity = 3.0;
        lane.inventory = InventoryState::default();
        lane.recovery_last_error = None;
        sync_runtime_fields_from_inventory(&mut lane, Some(100.0));
        assert!(
            lane.recovery_last_error.is_none(),
            "a normal close (cached quantity non-zero at entry) is not an anomaly"
        );
    }

    #[derive(Default)]
    struct StubPollingEngine {
        context: Option<LanePollingContext>,
    }

    impl LanePollingEngine for StubPollingEngine {
        type Error = String;
        type Outcome = ();

        fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn polling_context(&self, _instance_id: &str) -> Result<LanePollingContext, Self::Error> {
            self.context
                .clone()
                .ok_or_else(|| "no polling context configured".to_owned())
        }

        fn invariant_violation(&self, instance_id: &str, reason: &str) -> Self::Error {
            format!("invariant violation for `{instance_id}`: {reason}")
        }

        fn replay_confirmed_bar(
            &mut self,
            _instance_id: &str,
            _bar: &OhlcvBar,
            _mode: ConfirmedBarReplayMode,
        ) -> Result<ConfirmedBarReplayResult<Self::Outcome>, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn fetch_latest_bar(
            &mut self,
            _instance_id: &str,
            _account_id: &str,
            _data_connector: &str,
            _symbol: &str,
            _timeframe: Timeframe,
        ) -> Result<OhlcvBar, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn fetch_latest_confirmed_bar_timestamp(
            &mut self,
            _instance_id: &str,
            _account_id: &str,
            _data_connector: &str,
            _symbol: &str,
            _timeframe: Timeframe,
        ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn fetch_confirmed_bars_range(
            &mut self,
            _instance_id: &str,
            _account_id: &str,
            _data_connector: &str,
            _symbol: &str,
            _timeframe: Timeframe,
            _start_after: Option<chrono::DateTime<chrono::Utc>>,
            _end_at: chrono::DateTime<chrono::Utc>,
            _limit: usize,
        ) -> Result<ConfirmedBarPage, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn start_lane_recovery(
            &mut self,
            _instance_id: &str,
            _target: chrono::DateTime<chrono::Utc>,
            _now_ms: i64,
        ) -> Result<(), Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn complete_lane_recovery(
            &mut self,
            _instance_id: &str,
            _reason: &str,
        ) -> Result<(), Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn mark_lane_out_of_sync(
            &mut self,
            _instance_id: &str,
            _reason: &str,
        ) -> Result<(), Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn last_dispatched_bar_timestamp(
            &self,
            _instance_id: &str,
        ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn apply_recovery_page(
            &mut self,
            _instance_id: &str,
            _bars: &[OhlcvBar],
        ) -> Result<usize, Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn record_recovery_page_applied(
            &mut self,
            _instance_id: &str,
            _detail: RecoveryPageApplied,
        ) -> Result<(), Self::Error> {
            unimplemented!("not exercised by this test")
        }

        fn record_recovery_no_progress(
            &mut self,
            _instance_id: &str,
            _target: chrono::DateTime<chrono::Utc>,
            _exhausted: bool,
        ) -> Result<(), Self::Error> {
            unimplemented!("not exercised by this test")
        }
    }

    #[test]
    fn advance_lane_polling_once_returns_err_when_catching_up_without_target() {
        let mut engine = StubPollingEngine {
            context: Some(LanePollingContext {
                account_id: "acct".to_owned(),
                data_connector: "paper".to_owned(),
                symbol: "AAPL".to_owned(),
                timeframe: Timeframe::M1,
                // Invariant violation: CatchingUp with no recovery target.
                recovery_state: LaneRecoveryState::CatchingUp,
                last_dispatched: None,
                recovery_target: None,
            }),
        };

        let result = advance_lane_polling_once(&mut engine, "bot-a", 2, 4, 0);
        let error =
            result.expect_err("missing recovery target must surface as an error, not a panic");
        assert!(
            error.contains("CatchingUp without a recovery target"),
            "unexpected error message: {error}"
        );
    }

    #[derive(Debug)]
    #[allow(clippy::struct_excessive_bools)]
    struct MockManualCloseEngine {
        context: ManualCloseContext,
        remote_has_position: bool,
        latest_bar: OhlcvBar,
        signal_outcome: ManualCloseSignalOutcome,
        sync_remote_position_called: bool,
        fetch_latest_bar_called: bool,
        process_manual_close_signal_called: bool,
    }

    impl Default for MockManualCloseEngine {
        fn default() -> Self {
            Self {
                context: ManualCloseContext {
                    bot_id: "bot-a".to_owned(),
                    account_id: "acct".to_owned(),
                    reconciliation_remote_snapshot: false,
                    has_local_position: false,
                },
                remote_has_position: false,
                latest_bar: test_bar(100.0),
                signal_outcome: ManualCloseSignalOutcome {
                    intent: TradeIntent::NoOp,
                    risk: ManualCloseSignalRisk::Allowed,
                },
                sync_remote_position_called: false,
                fetch_latest_bar_called: false,
                process_manual_close_signal_called: false,
            }
        }
    }

    impl LaneManualOpsEngine for MockManualCloseEngine {
        type Error = &'static str;

        fn manual_close_context(
            &self,
            _instance_id: &str,
        ) -> Result<ManualCloseContext, Self::Error> {
            Ok(self.context.clone())
        }

        fn sync_remote_position_for_manual_close(
            &mut self,
            _instance_id: &str,
            _account_id: &str,
        ) -> Result<bool, Self::Error> {
            self.sync_remote_position_called = true;
            Ok(self.remote_has_position)
        }

        fn fetch_latest_bar_for_manual_close(
            &mut self,
            _instance_id: &str,
        ) -> Result<OhlcvBar, Self::Error> {
            self.fetch_latest_bar_called = true;
            Ok(self.latest_bar.clone())
        }

        fn process_manual_close_signal(
            &mut self,
            _instance_id: &str,
            price: f64,
            timestamp: chrono::DateTime<chrono::Utc>,
        ) -> Result<ManualCloseSignalOutcome, Self::Error> {
            self.process_manual_close_signal_called = true;
            assert!((price - self.latest_bar.close).abs() < 1e-9);
            assert_eq!(timestamp, self.latest_bar.timestamp);
            Ok(self.signal_outcome.clone())
        }
    }

    fn test_lane_runtime() -> super::LaneRuntime {
        let config = InstanceConfig {
            id: "bot-a".to_owned(),
            enabled: true,
            market: MarketType::Equities,
            account: "acct".to_owned(),
            execution_connector: "paper".to_owned(),
            data_connector: "paper".to_owned(),
            timeframe: Timeframe::M1,
            symbols: vec!["AAPL".to_owned()],
            budget: BudgetConfig { pct: 25.0 },
            indicators: vec![],
            strategy: "single_indicator_signal".to_owned(),
            signal_mode: SignalMode::ConfirmedOnly,
            execution_constraints: ExecutionConstraintsConfig::default(),
            polling_enabled: true,
            polling_interval_ms: 60_000,
            risk: InstanceRiskConfig {
                profile: "default".to_owned(),
                overrides: RiskOverrides::default(),
            },
            warmup_target_bars: Some(0),
            allow_live: false,
        };

        super::LaneRuntime {
            config: config.clone(),
            lane_symbol: "AAPL".to_owned(),
            execution_mode: ExecutionMode::Paper,
            state: LaneRuntimeState::Stopped,
            resume_after_startup_reconcile: false,
            indicators: Vec::new(),
            strategy: build_runtime_strategy(&config).expect("strategy should build"),
            bar_builder: openticker_data::BarBuilder::new("AAPL".to_owned(), Timeframe::M1),
            risk_limits: RiskLimits {
                max_daily_loss_pct: 5.0,
                max_open_positions: 5,
                max_order_notional_usd: 1_000.0,
                max_spread_bps: 20,
                max_slippage_bps: 20,
                stale_data_ms: 3_000,
                cooldown_after_reject_ms: 1_000,
            },
            target_order_notional_usd: 100.0,
            inventory: InventoryState::default(),
            has_position: false,
            position_quantity: 0.0,
            position_notional_usd: 0.0,
            entry_price: None,
            realized_pnl_usd: 0.0,
            daily_loss_pct_accumulated: 0.0,
            last_loss_reset_date: None,
            cooldown_until_ms: None,
            reconciliation_blocked: false,
            remote_net_qty: None,
            aggregate_managed_qty: 0.0,
            external_delta_qty: None,
            managed_remote_open_orders: 0,
            external_remote_open_orders: 0,
            warmup: InstanceWarmupState::new(0),
            recovery_state: LaneRecoveryState::Healthy,
            recovery_started_at_ms: None,
            recovery_target_timestamp: None,
            recovery_last_progress_timestamp: None,
            recovery_last_error: None,
            recovery_consecutive_no_progress_cycles: 0,
            last_recovered_at_timestamp: None,
            last_dispatched_bar_timestamp: None,
            last_stream_update: None,
            connector_execution_constraints: None,
            connector_fractional_entry_supported: None,
            connector_execution_constraints_initialized: false,
        }
    }

    fn test_bar(close: f64) -> OhlcvBar {
        test_bar_at(close, 0)
    }

    fn test_bar_at(close: f64, minute_offset: i64) -> OhlcvBar {
        OhlcvBar {
            timestamp: chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .expect("timestamp should parse")
                .with_timezone(&chrono::Utc)
                + chrono::Duration::minutes(minute_offset),
            open: close,
            high: close,
            low: close,
            close,
            volume: 0.0,
        }
    }
}
