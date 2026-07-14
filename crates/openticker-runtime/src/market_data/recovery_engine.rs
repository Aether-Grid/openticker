use super::pending_provider_events::{
    GatewayFetchFailure, PendingProviderEvent, append_pending_provider_events,
    gateway_fetch_confirmed_bars_range_with_events, gateway_fetch_latest_bar_with_events,
    gateway_fetch_latest_confirmed_bar_timestamp_with_events,
};
use crate::{LaneRecoveryState, OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, unix_now_ms};
use chrono::{DateTime, Utc};
use openticker_connectors::ConfirmedBarPage;
use openticker_gateway::Gateway;
pub(crate) use openticker_lane::ConfirmedBarReplayMode;
use openticker_lane::{LanePollingContext, RecoveryPageApplied, validate_recovery_bars};

pub(crate) const RECOVERY_PAGE_LIMIT: usize = 200;
pub(crate) const MAX_RECOVERY_PAGES_PER_CYCLE: usize = 4;

pub(crate) type LanePollingAdvance = openticker_lane::LanePollingAdvance<ProcessBarOutcome>;
pub(crate) type ConfirmedBarReplayResult =
    openticker_lane::ConfirmedBarReplayResult<ProcessBarOutcome>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LatestBarPlanMode {
    BootstrapTradable,
    DuplicateOnly,
}

#[derive(Debug, Clone)]
pub(crate) enum LanePollPlan {
    LatestBar {
        lane_id: String,
        account_id: String,
        data_connector: String,
        symbol: String,
        timeframe: crate::Timeframe,
        mode: LatestBarPlanMode,
        expected_last_dispatched: Option<DateTime<Utc>>,
    },
    LatestConfirmedTarget {
        lane_id: String,
        account_id: String,
        data_connector: String,
        symbol: String,
        timeframe: crate::Timeframe,
        last_dispatched: DateTime<Utc>,
        recovery_state: LaneRecoveryState,
        page_limit: usize,
        now_ms: i64,
    },
    ConfirmedRange {
        lane_id: String,
        account_id: String,
        data_connector: String,
        symbol: String,
        timeframe: crate::Timeframe,
        start_after: Option<DateTime<Utc>>,
        target: DateTime<Utc>,
        page_limit: usize,
        now_ms: i64,
        start_recovery: bool,
        allow_live_confirmed_shortcut: bool,
    },
}

#[derive(Debug)]
pub(crate) enum LanePollFetchResult {
    LatestBar(OhlcvBar),
    LatestConfirmedTarget(Option<DateTime<Utc>>),
    ConfirmedRange(ConfirmedBarPage),
}

#[derive(Debug)]
pub(crate) struct LanePollExecution {
    pub(crate) fetch_result: LanePollFetchResult,
    pub(crate) provider_events: Vec<PendingProviderEvent>,
}

#[derive(Debug, Default)]
pub(crate) struct LanePollApplyOutcome {
    pub(crate) advance: LanePollingAdvance,
    pub(crate) next_plan: Option<LanePollPlan>,
}

impl Runtime {
    pub(crate) fn lane_polling_context(
        &self,
        instance_id: &str,
    ) -> Result<LanePollingContext, ServiceError> {
        if self.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        let (account_id, data_connector, symbol, timeframe) =
            self.manual_poll_target_for_instance(instance_id, "poll_instance_once")?;
        let instance = self.instance(instance_id)?;
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

    pub(crate) fn plan_lane_polling(
        &self,
        instance_id: &str,
        page_limit: usize,
        now_ms: i64,
    ) -> Result<LanePollPlan, ServiceError> {
        let context = self.lane_polling_context(instance_id)?;

        if context.recovery_state == LaneRecoveryState::CatchingUp {
            let target = context
                .recovery_target
                .expect("polling context must include a target while catching up");
            return Ok(LanePollPlan::ConfirmedRange {
                lane_id: instance_id.to_owned(),
                account_id: context.account_id,
                data_connector: context.data_connector,
                symbol: context.symbol,
                timeframe: context.timeframe,
                start_after: context.last_dispatched,
                target,
                page_limit: page_limit.max(1),
                now_ms,
                start_recovery: false,
                allow_live_confirmed_shortcut: false,
            });
        }

        if context.last_dispatched.is_none() {
            return Ok(LanePollPlan::LatestBar {
                lane_id: instance_id.to_owned(),
                account_id: context.account_id,
                data_connector: context.data_connector,
                symbol: context.symbol,
                timeframe: context.timeframe,
                mode: LatestBarPlanMode::BootstrapTradable,
                expected_last_dispatched: None,
            });
        }

        Ok(LanePollPlan::LatestConfirmedTarget {
            lane_id: instance_id.to_owned(),
            account_id: context.account_id,
            data_connector: context.data_connector,
            symbol: context.symbol,
            timeframe: context.timeframe,
            last_dispatched: context.last_dispatched.expect("checked above"),
            recovery_state: context.recovery_state,
            page_limit,
            now_ms,
        })
    }

    pub(crate) fn execute_lane_poll_plan(
        gateway: &Gateway,
        plan: &LanePollPlan,
    ) -> Result<LanePollExecution, GatewayFetchFailure> {
        match plan {
            LanePollPlan::LatestBar {
                lane_id,
                account_id,
                data_connector,
                symbol,
                timeframe,
                ..
            } => {
                let (bar, provider_events) = gateway_fetch_latest_bar_with_events(
                    gateway,
                    lane_id,
                    account_id,
                    data_connector,
                    symbol,
                    *timeframe,
                )?;
                Ok(LanePollExecution {
                    fetch_result: LanePollFetchResult::LatestBar(bar),
                    provider_events,
                })
            }
            LanePollPlan::LatestConfirmedTarget {
                lane_id,
                account_id,
                data_connector,
                symbol,
                timeframe,
                ..
            } => {
                let (timestamp, provider_events) =
                    gateway_fetch_latest_confirmed_bar_timestamp_with_events(
                        gateway,
                        lane_id,
                        account_id,
                        data_connector,
                        symbol,
                        *timeframe,
                    )?;
                Ok(LanePollExecution {
                    fetch_result: LanePollFetchResult::LatestConfirmedTarget(timestamp),
                    provider_events,
                })
            }
            LanePollPlan::ConfirmedRange {
                lane_id,
                account_id,
                data_connector,
                symbol,
                timeframe,
                start_after,
                target,
                page_limit,
                ..
            } => {
                let (page, provider_events) = gateway_fetch_confirmed_bars_range_with_events(
                    gateway,
                    lane_id,
                    account_id,
                    data_connector,
                    symbol,
                    *timeframe,
                    *start_after,
                    *target,
                    *page_limit,
                )?;
                Ok(LanePollExecution {
                    fetch_result: LanePollFetchResult::ConfirmedRange(page),
                    provider_events,
                })
            }
        }
    }

    pub(crate) fn apply_lane_poll_plan(
        &mut self,
        plan: LanePollPlan,
        execution: LanePollExecution,
    ) -> Result<LanePollApplyOutcome, ServiceError> {
        append_pending_provider_events(self, &execution.provider_events)?;

        match (plan, execution.fetch_result) {
            (
                LanePollPlan::LatestBar {
                    lane_id,
                    account_id,
                    data_connector,
                    symbol,
                    timeframe,
                    mode,
                    expected_last_dispatched,
                },
                LanePollFetchResult::LatestBar(bar),
            ) => self.apply_latest_bar_plan(
                &lane_id,
                &account_id,
                &data_connector,
                &symbol,
                timeframe,
                mode,
                expected_last_dispatched,
                bar,
            ),
            (
                LanePollPlan::LatestConfirmedTarget {
                    lane_id,
                    account_id,
                    data_connector,
                    symbol,
                    timeframe,
                    last_dispatched,
                    recovery_state,
                    page_limit,
                    now_ms,
                },
                LanePollFetchResult::LatestConfirmedTarget(latest_target),
            ) => self.apply_latest_confirmed_target_plan(
                &lane_id,
                &account_id,
                &data_connector,
                &symbol,
                timeframe,
                last_dispatched,
                recovery_state,
                page_limit,
                now_ms,
                latest_target,
            ),
            (
                LanePollPlan::ConfirmedRange {
                    lane_id,
                    account_id,
                    data_connector,
                    symbol,
                    timeframe,
                    start_after,
                    target,
                    page_limit,
                    now_ms,
                    start_recovery,
                    allow_live_confirmed_shortcut,
                },
                LanePollFetchResult::ConfirmedRange(page),
            ) => self.apply_confirmed_range_plan(
                &lane_id,
                &account_id,
                &data_connector,
                &symbol,
                timeframe,
                start_after,
                target,
                page_limit,
                now_ms,
                start_recovery,
                allow_live_confirmed_shortcut,
                &page,
            ),
            _ => Err(ServiceError::InvalidConfiguration(
                "lane polling plan/fetch mismatch".to_owned(),
            )),
        }
    }

    pub(crate) fn advance_lane_polling_once(
        &mut self,
        instance_id: &str,
        page_limit: usize,
        max_pages_per_cycle: usize,
    ) -> Result<LanePollingAdvance, ServiceError> {
        let gateway = self.connector_gateway_snapshot();
        let mut advance = LanePollingAdvance::default();
        let mut next_plan = Some(self.plan_lane_polling(instance_id, page_limit, unix_now_ms())?);
        let mut recovery_pages = 0usize;

        while let Some(plan) = next_plan.take() {
            let is_recovery_page = matches!(plan, LanePollPlan::ConfirmedRange { .. });
            if is_recovery_page && recovery_pages >= max_pages_per_cycle.max(1) {
                break;
            }

            let execution = match Runtime::execute_lane_poll_plan(&gateway, &plan) {
                Ok(execution) => execution,
                Err(failure) => {
                    append_pending_provider_events(self, &failure.provider_events)?;
                    return Err(failure.error);
                }
            };
            let outcome = self.apply_lane_poll_plan(plan, execution)?;
            if is_recovery_page {
                recovery_pages = recovery_pages.saturating_add(1);
            }
            advance.recorded_bars.extend(outcome.advance.recorded_bars);
            advance.outcomes.extend(outcome.advance.outcomes);
            next_plan = outcome.next_plan;
        }

        Ok(advance)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_latest_bar_plan(
        &mut self,
        lane_id: &str,
        _account_id: &str,
        _data_connector: &str,
        _symbol: &str,
        _timeframe: crate::Timeframe,
        mode: LatestBarPlanMode,
        expected_last_dispatched: Option<DateTime<Utc>>,
        bar: OhlcvBar,
    ) -> Result<LanePollApplyOutcome, ServiceError> {
        let current_last_dispatched = self.instance(lane_id)?.last_dispatched_bar_timestamp;
        if current_last_dispatched != expected_last_dispatched {
            return Ok(LanePollApplyOutcome::default());
        }

        let mut outcome = LanePollApplyOutcome::default();
        match mode {
            LatestBarPlanMode::BootstrapTradable => {
                let replay = self.replay_confirmed_bar_for_lane(
                    lane_id,
                    &bar,
                    ConfirmedBarReplayMode::LiveConfirmedTradable,
                )?;
                if replay.applied {
                    outcome.advance.recorded_bars.push(bar);
                }
                if let Some(process_outcome) = replay.outcome {
                    outcome.advance.outcomes.push(process_outcome);
                }
            }
            LatestBarPlanMode::DuplicateOnly => {
                if current_last_dispatched.is_some_and(|timestamp| timestamp == bar.timestamp) {
                    outcome.advance.recorded_bars.push(bar);
                }
            }
        }

        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_latest_confirmed_target_plan(
        &mut self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
        last_dispatched: DateTime<Utc>,
        recovery_state: LaneRecoveryState,
        page_limit: usize,
        now_ms: i64,
        latest_target: Option<DateTime<Utc>>,
    ) -> Result<LanePollApplyOutcome, ServiceError> {
        let instance = self.instance(lane_id)?;
        if instance.last_dispatched_bar_timestamp != Some(last_dispatched)
            || instance.recovery_state != recovery_state
        {
            return Ok(LanePollApplyOutcome::default());
        }

        let Some(latest_target) = latest_target else {
            return Ok(LanePollApplyOutcome::default());
        };

        if latest_target <= last_dispatched {
            if recovery_state != LaneRecoveryState::Healthy {
                self.complete_lane_recovery(lane_id, "target already current")?;
            }
            return Ok(LanePollApplyOutcome {
                advance: LanePollingAdvance::default(),
                next_plan: Some(LanePollPlan::LatestBar {
                    lane_id: lane_id.to_owned(),
                    account_id: account_id.to_owned(),
                    data_connector: data_connector.to_owned(),
                    symbol: symbol.to_owned(),
                    timeframe,
                    mode: LatestBarPlanMode::DuplicateOnly,
                    expected_last_dispatched: Some(last_dispatched),
                }),
            });
        }

        Ok(LanePollApplyOutcome {
            advance: LanePollingAdvance::default(),
            next_plan: Some(LanePollPlan::ConfirmedRange {
                lane_id: lane_id.to_owned(),
                account_id: account_id.to_owned(),
                data_connector: data_connector.to_owned(),
                symbol: symbol.to_owned(),
                timeframe,
                start_after: Some(last_dispatched),
                target: latest_target,
                page_limit: page_limit.max(2),
                now_ms,
                start_recovery: true,
                allow_live_confirmed_shortcut: recovery_state == LaneRecoveryState::Healthy,
            }),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_confirmed_range_plan(
        &mut self,
        lane_id: &str,
        account_id: &str,
        data_connector: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
        start_after: Option<DateTime<Utc>>,
        target: DateTime<Utc>,
        page_limit: usize,
        now_ms: i64,
        start_recovery: bool,
        allow_live_confirmed_shortcut: bool,
        page: &ConfirmedBarPage,
    ) -> Result<LanePollApplyOutcome, ServiceError> {
        let current_last_dispatched = self.instance(lane_id)?.last_dispatched_bar_timestamp;
        if current_last_dispatched != start_after {
            return Ok(LanePollApplyOutcome::default());
        }
        let Some(start_after_timestamp) = current_last_dispatched else {
            self.mark_lane_out_of_sync(
                lane_id,
                "recovery lost the authoritative last dispatched timestamp",
            )?;
            return Ok(LanePollApplyOutcome::default());
        };

        if let Err(reason) = validate_recovery_bars(Some(start_after_timestamp), target, &page.bars)
        {
            self.mark_lane_out_of_sync(lane_id, &reason)?;
            return Ok(LanePollApplyOutcome::default());
        }

        if allow_live_confirmed_shortcut
            && page.exhausted
            && page.bars.len() == 1
            && page.bars[0].timestamp == target
        {
            let latest_bar = page.bars[0].clone();
            let replay = self.replay_confirmed_bar_for_lane(
                lane_id,
                &latest_bar,
                ConfirmedBarReplayMode::LiveConfirmedTradable,
            )?;
            let mut outcome = LanePollApplyOutcome::default();
            if replay.applied {
                outcome.advance.recorded_bars.push(latest_bar);
            }
            if let Some(process_outcome) = replay.outcome {
                outcome.advance.outcomes.push(process_outcome);
            }
            return Ok(outcome);
        }

        if start_recovery {
            self.start_lane_recovery(lane_id, target, now_ms)?;
        }

        if page.bars.is_empty() {
            self.record_recovery_no_progress(lane_id, target, page.exhausted)?;
            return Ok(LanePollApplyOutcome::default());
        }

        let first_timestamp = page.bars.first().map(|bar| bar.timestamp);
        let last_timestamp = page.bars.last().map(|bar| bar.timestamp);
        let bars_applied = self.apply_recovery_page(lane_id, &page.bars)?;
        let mut outcome = LanePollApplyOutcome::default();
        outcome.advance.recorded_bars.extend(page.bars.clone());
        self.record_recovery_page_applied(
            lane_id,
            &RecoveryPageApplied {
                account_id: account_id.to_owned(),
                symbol: symbol.to_owned(),
                timeframe,
                recovery_target_timestamp: target,
                first_bar_timestamp: first_timestamp,
                last_bar_timestamp: last_timestamp,
                bars_applied,
                page_limit,
                exhausted: page.exhausted,
            },
        )?;

        let updated_last_dispatched = self.instance(lane_id)?.last_dispatched_bar_timestamp;
        if updated_last_dispatched.is_some_and(|timestamp| timestamp >= target) {
            self.complete_lane_recovery(lane_id, "recovery target reached")?;
            return Ok(outcome);
        }

        if page.exhausted {
            self.mark_lane_out_of_sync(
                lane_id,
                &format!(
                    "confirmed history exhausted before reaching recovery target {}",
                    target.to_rfc3339()
                ),
            )?;
            return Ok(outcome);
        }

        let Some(next_start_after) = updated_last_dispatched else {
            self.mark_lane_out_of_sync(
                lane_id,
                "recovery lost the authoritative last dispatched timestamp",
            )?;
            return Ok(outcome);
        };

        outcome.next_plan = Some(LanePollPlan::ConfirmedRange {
            lane_id: lane_id.to_owned(),
            account_id: account_id.to_owned(),
            data_connector: data_connector.to_owned(),
            symbol: symbol.to_owned(),
            timeframe,
            start_after: Some(next_start_after),
            target,
            page_limit: page_limit.max(1),
            now_ms,
            start_recovery: false,
            allow_live_confirmed_shortcut: false,
        });

        Ok(outcome)
    }
}
