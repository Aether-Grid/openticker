use openticker_core::{IndicatorSignal, OhlcvBar, SignalPhase};
use openticker_trace::{CycleTriggerKind, TraceIdentity};
use std::time::{Duration, Instant};

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
