use crate::state::{LaneRecoveryState, LaneRuntime};
use openticker_core::{OhlcvBar, Timeframe};

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
pub struct RecoveryNoProgressState {
    pub cycles: u32,
    pub should_fail: bool,
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
