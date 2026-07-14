use openticker_core::{OhlcvBar, SignalPhase};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupAdvance {
    pub loaded_bars: usize,
    pub required_bars: usize,
    pub became_ready: bool,
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
