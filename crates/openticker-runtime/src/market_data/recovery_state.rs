use crate::{LaneRecoveryState, OhlcvBar, Runtime, ServiceError};
use openticker_lane::{
    ConfirmedBarReplayMode, RecoveryPageApplied,
    apply_state_only_confirmed_bar as apply_lane_state_only_confirmed_bar,
    complete_lane_recovery_state, mark_lane_out_of_sync_state, mark_recovery_page_progress,
    record_recovery_no_progress_state, start_lane_recovery_state,
};

pub(super) const MAX_RECOVERY_NO_PROGRESS_CYCLES: u32 = 3;

impl Runtime {
    pub(super) fn apply_state_only_confirmed_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
    ) -> Result<bool, ServiceError> {
        let instance = self.instance_mut(instance_id)?;
        Ok(apply_lane_state_only_confirmed_bar(instance, bar))
    }

    pub(super) fn apply_recovery_page(
        &mut self,
        instance_id: &str,
        bars: &[OhlcvBar],
    ) -> Result<usize, ServiceError> {
        let mut applied = 0usize;
        for bar in bars {
            let replay = self.replay_confirmed_bar_for_lane(
                instance_id,
                bar,
                ConfirmedBarReplayMode::RecoveryStateOnly,
            )?;
            if replay.applied {
                applied = applied.saturating_add(1);
            }
        }

        if let Some(last_bar) = bars.last() {
            let instance = self.instance_mut(instance_id)?;
            mark_recovery_page_progress(instance, last_bar.timestamp);
        }

        Ok(applied)
    }

    pub(super) fn record_recovery_page_applied(
        &mut self,
        instance_id: &str,
        detail: &RecoveryPageApplied,
    ) -> Result<(), ServiceError> {
        self.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.recovery.page_applied",
            serde_json::json!({
                "account": detail.account_id,
                "symbol": detail.symbol,
                "timeframe": detail.timeframe,
                "recovery_state": self.instance(instance_id)?.recovery_state,
                "recovery_target_timestamp": detail.recovery_target_timestamp.to_rfc3339(),
                "first_bar_timestamp": detail.first_bar_timestamp.map(|value| value.to_rfc3339()),
                "last_bar_timestamp": detail.last_bar_timestamp.map(|value| value.to_rfc3339()),
                "bars_applied": detail.bars_applied,
                "page_limit": detail.page_limit,
                "exhausted": detail.exhausted,
            })
            .to_string(),
        )
    }

    pub(super) fn start_lane_recovery(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        now_ms: i64,
    ) -> Result<(), ServiceError> {
        let kind = {
            let instance = self.instance_mut(instance_id)?;
            start_lane_recovery_state(instance, target, now_ms)
        };
        self.append_runtime_event(
            "poll",
            Some(instance_id),
            kind.event_kind(),
            serde_json::json!({
                "recovery_state": LaneRecoveryState::CatchingUp,
                "recovery_target_timestamp": target.to_rfc3339(),
                "recovery_started_at_ms": now_ms,
            })
            .to_string(),
        )
    }

    pub(super) fn complete_lane_recovery(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), ServiceError> {
        let last_progress_timestamp = {
            let instance = self.instance_mut(instance_id)?;
            complete_lane_recovery_state(instance)
        };
        self.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.recovery.completed",
            serde_json::json!({
                "recovery_state": LaneRecoveryState::Healthy,
                "recovery_last_progress_timestamp": last_progress_timestamp
                    .map(|value| value.to_rfc3339()),
                "reason": reason,
            })
            .to_string(),
        )
    }

    pub(super) fn mark_lane_out_of_sync(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), ServiceError> {
        {
            let instance = self.instance_mut(instance_id)?;
            mark_lane_out_of_sync_state(instance, reason);
        }
        self.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.recovery.failed",
            serde_json::json!({
                "recovery_state": LaneRecoveryState::OutOfSync,
                "failure_reason": reason,
            })
            .to_string(),
        )
    }

    pub(super) fn record_recovery_no_progress(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        exhausted: bool,
    ) -> Result<(), ServiceError> {
        let no_progress = {
            let instance = self.instance_mut(instance_id)?;
            record_recovery_no_progress_state(instance, exhausted, MAX_RECOVERY_NO_PROGRESS_CYCLES)
        };

        self.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.recovery.page_applied",
            serde_json::json!({
                "recovery_state": self.instance(instance_id)?.recovery_state,
                "recovery_target_timestamp": target.to_rfc3339(),
                "bars_applied": 0,
                "exhausted": exhausted,
            })
            .to_string(),
        )?;

        if no_progress.should_fail {
            let cycles = no_progress.cycles;
            let reason = if exhausted {
                format!(
                    "connector exhausted confirmed history before reaching recovery target {}",
                    target.to_rfc3339()
                )
            } else {
                format!(
                    "recovery made no progress for {} consecutive cycles toward {}",
                    cycles,
                    target.to_rfc3339()
                )
            };
            self.mark_lane_out_of_sync(instance_id, &reason)?;
        }

        Ok(())
    }
}
