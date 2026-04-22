use crate::{LaneRecoveryState, OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, unix_now_ms};
pub(crate) use openticker_lane::ConfirmedBarReplayMode;
use openticker_lane::{
    LanePollingContext, LanePollingEngine, RecoveryPageApplied,
    advance_lane_polling_once as advance_lane_polling_cycle,
    apply_state_only_confirmed_bar as apply_lane_state_only_confirmed_bar,
    complete_lane_recovery_state, mark_lane_out_of_sync_state, mark_recovery_page_progress,
    record_recovery_no_progress_state, start_lane_recovery_state,
};

pub(crate) const RECOVERY_PAGE_LIMIT: usize = 200;
pub(crate) const MAX_RECOVERY_PAGES_PER_CYCLE: usize = 4;
pub(super) const MAX_RECOVERY_NO_PROGRESS_CYCLES: u32 = 3;

pub(crate) type LanePollingAdvance = openticker_lane::LanePollingAdvance<ProcessBarOutcome>;
pub(crate) type ConfirmedBarReplayResult =
    openticker_lane::ConfirmedBarReplayResult<ProcessBarOutcome>;

pub(super) struct RuntimeLanePollingEngine<'a> {
    pub(super) runtime: &'a mut Runtime,
}

impl LanePollingEngine for RuntimeLanePollingEngine<'_> {
    type Error = ServiceError;
    type Outcome = ProcessBarOutcome;

    fn ensure_kill_switch_inactive(&self) -> Result<(), Self::Error> {
        if self.runtime.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        Ok(())
    }

    fn polling_context(&self, instance_id: &str) -> Result<LanePollingContext, Self::Error> {
        let (account_id, data_connector, symbol, timeframe) = self
            .runtime
            .manual_poll_target_for_instance(instance_id, "poll_instance_once")?;
        let instance = self.runtime.instance(instance_id)?;
        if instance.recovery_state == LaneRecoveryState::CatchingUp
            && instance.recovery_target_timestamp.is_none()
        {
            return Err(ServiceError::InvalidConfiguration(format!(
                "lane `{instance_id}` entered recovery without a target timestamp"
            )));
        }

        Ok(LanePollingContext {
            account_id,
            data_connector,
            symbol,
            timeframe,
            recovery_state: instance.recovery_state,
            last_dispatched: instance.last_dispatched_bar_timestamp,
            recovery_target: instance.recovery_target_timestamp,
        })
    }

    fn replay_confirmed_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        mode: ConfirmedBarReplayMode,
    ) -> Result<ConfirmedBarReplayResult, Self::Error> {
        self.runtime
            .replay_confirmed_bar_for_lane(instance_id, bar, mode)
    }

    fn fetch_latest_bar(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
    ) -> Result<OhlcvBar, Self::Error> {
        self.runtime.connector_gateway().fetch_latest_bar(
            instance_id,
            account_id,
            data_connector,
            symbol,
            timeframe,
        )
    }

    fn fetch_latest_confirmed_bar_timestamp(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
        self.runtime
            .connector_gateway()
            .fetch_latest_confirmed_bar_timestamp(
                instance_id,
                account_id,
                data_connector,
                symbol,
                timeframe,
            )
    }

    fn fetch_confirmed_bars_range(
        &mut self,
        instance_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
        start_after: Option<chrono::DateTime<chrono::Utc>>,
        end_at: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<openticker_connectors::ConfirmedBarPage, Self::Error> {
        self.runtime.connector_gateway().fetch_confirmed_bars_range(
            instance_id,
            account_id,
            data_connector,
            symbol,
            timeframe,
            start_after,
            end_at,
            limit,
        )
    }

    fn start_lane_recovery(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        now_ms: i64,
    ) -> Result<(), Self::Error> {
        self.runtime
            .start_lane_recovery(instance_id, target, now_ms)
    }

    fn complete_lane_recovery(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), Self::Error> {
        self.runtime.complete_lane_recovery(instance_id, reason)
    }

    fn mark_lane_out_of_sync(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), Self::Error> {
        self.runtime.mark_lane_out_of_sync(instance_id, reason)
    }

    fn last_dispatched_bar_timestamp(
        &self,
        instance_id: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, Self::Error> {
        Ok(self
            .runtime
            .instance(instance_id)?
            .last_dispatched_bar_timestamp)
    }

    fn apply_recovery_page(
        &mut self,
        instance_id: &str,
        bars: &[OhlcvBar],
    ) -> Result<usize, Self::Error> {
        self.runtime.apply_recovery_page(instance_id, bars)
    }

    fn record_recovery_page_applied(
        &mut self,
        instance_id: &str,
        detail: RecoveryPageApplied,
    ) -> Result<(), Self::Error> {
        self.runtime.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.recovery.page_applied",
            serde_json::json!({
                "account": detail.account_id,
                "symbol": detail.symbol,
                "timeframe": detail.timeframe,
                "recovery_state": self.runtime.instance(instance_id)?.recovery_state,
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

    fn record_recovery_no_progress(
        &mut self,
        instance_id: &str,
        target: chrono::DateTime<chrono::Utc>,
        exhausted: bool,
    ) -> Result<(), Self::Error> {
        self.runtime
            .record_recovery_no_progress(instance_id, target, exhausted)
    }
}

impl Runtime {
    pub(crate) fn advance_lane_polling_once(
        &mut self,
        instance_id: &str,
        page_limit: usize,
        max_pages_per_cycle: usize,
    ) -> Result<LanePollingAdvance, ServiceError> {
        advance_lane_polling_cycle(
            &mut RuntimeLanePollingEngine { runtime: self },
            instance_id,
            page_limit,
            max_pages_per_cycle,
            unix_now_ms(),
        )
    }

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
