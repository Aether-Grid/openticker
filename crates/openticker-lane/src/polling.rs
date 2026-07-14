use crate::recovery::{RecoveryPageApplied, validate_recovery_bars};
use crate::state::LaneRecoveryState;
use openticker_connectors::ConfirmedBarPage;
use openticker_core::{OhlcvBar, Timeframe};

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
