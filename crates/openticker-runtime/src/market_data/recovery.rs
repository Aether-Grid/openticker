use super::recovery_engine::ConfirmedBarReplayResult;
pub(crate) use super::recovery_engine::{
    ConfirmedBarReplayMode, LanePollingAdvance, MAX_RECOVERY_PAGES_PER_CYCLE, RECOVERY_PAGE_LIMIT,
};
use crate::{
    LaneRecoveryState, OhlcvBar, ProcessBarOutcome, ProcessBarRisk, Runtime, ServiceError,
    SignalPhase, TradeIntent,
};

impl Runtime {
    pub(crate) fn process_pending_recovery_bar(
        &mut self,
        instance_id: &str,
        phase: SignalPhase,
    ) -> Result<Option<ProcessBarOutcome>, ServiceError> {
        let recovery_pending = {
            let instance = self.instance(instance_id)?;
            instance.recovery_state != LaneRecoveryState::Healthy
        };
        if !recovery_pending {
            return Ok(None);
        }

        let instance = self.instance(instance_id)?;
        let has_position = instance.has_position;
        let bot_id = instance.config.id.clone();
        Ok(Some(ProcessBarOutcome {
            instance_id: bot_id.clone(),
            bot_id,
            symbol: instance.lane_symbol.clone(),
            phase,
            signal: crate::IndicatorSignal::None,
            signal_metadata: None,
            intent: TradeIntent::NoOp,
            strategy_rationale: Some("recovery_pending".to_owned()),
            has_position,
            risk: ProcessBarRisk::Allowed,
        }))
    }

    pub(crate) fn replay_confirmed_bar_for_lane(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        mode: ConfirmedBarReplayMode,
    ) -> Result<ConfirmedBarReplayResult, ServiceError> {
        match mode {
            ConfirmedBarReplayMode::WarmupSeed | ConfirmedBarReplayMode::RecoveryStateOnly => {
                Ok(ConfirmedBarReplayResult {
                    applied: self.apply_state_only_confirmed_bar(instance_id, bar)?,
                    outcome: None,
                })
            }
            ConfirmedBarReplayMode::LiveConfirmedTradable => {
                if self
                    .instance(instance_id)?
                    .last_dispatched_bar_timestamp
                    .is_some_and(|previous| previous >= bar.timestamp)
                {
                    return Ok(ConfirmedBarReplayResult {
                        applied: false,
                        outcome: None,
                    });
                }

                let outcome =
                    self.process_bar_for_lane(instance_id, bar, SignalPhase::Confirmed)?;
                Ok(ConfirmedBarReplayResult {
                    applied: true,
                    outcome: Some(outcome),
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
