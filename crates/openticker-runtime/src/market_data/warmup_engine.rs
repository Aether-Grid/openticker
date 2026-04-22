use super::recovery::ConfirmedBarReplayMode;
use crate::{
    IndicatorSignal, OhlcvBar, ProcessBarOutcome, ProcessBarRisk, Runtime, ServiceError,
    SignalPhase, TradeIntent,
};
use openticker_lane::{
    LaneWarmupContext, LaneWarmupEngine, WarmupProgressDetail,
    advance_warmup_state as advance_lane_warmup_state,
    record_warmup_failure as record_lane_warmup_failure,
};

pub(super) struct RuntimeLaneWarmupEngine<'a> {
    pub(super) runtime: &'a mut Runtime,
}

impl LaneWarmupEngine for RuntimeLaneWarmupEngine<'_> {
    type Error = ServiceError;
    type Outcome = ProcessBarOutcome;

    fn warmup_context(&self, instance_id: &str) -> Result<LaneWarmupContext, Self::Error> {
        let instance = self.runtime.instance(instance_id)?;
        Ok(LaneWarmupContext {
            required_bars: instance.warmup.required_bars,
            ready: instance.warmup.ready,
        })
    }

    fn fetch_recent_bars_for_warmup(
        &mut self,
        instance_id: &str,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, Self::Error> {
        self.runtime
            .fetch_recent_bars_for_instance(instance_id, limit)
            .map(|(_, bars)| bars)
    }

    fn replay_confirmed_warmup_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
    ) -> Result<bool, Self::Error> {
        self.runtime
            .replay_confirmed_bar_for_lane(instance_id, bar, ConfirmedBarReplayMode::WarmupSeed)
            .map(|result| result.applied)
    }

    fn advance_warmup_state(
        &mut self,
        instance_id: &str,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<openticker_lane::WarmupAdvance>, Self::Error> {
        let instance = self.runtime.instance_mut(instance_id)?;
        Ok(advance_lane_warmup_state(&mut instance.warmup, timestamp))
    }

    fn record_warmup_started(
        &mut self,
        instance_id: &str,
        source: &'static str,
        required_bars: usize,
    ) -> Result<(), Self::Error> {
        self.runtime.append_runtime_event(
            "warmup",
            Some(instance_id),
            "warmup.started",
            serde_json::json!({
                "source": source,
                "required_bars": required_bars,
            })
            .to_string(),
        )
    }

    fn record_warmup_failure(
        &mut self,
        instance_id: &str,
        detail: String,
    ) -> Result<(), Self::Error> {
        self.runtime.record_warmup_failure(instance_id, detail)
    }

    fn record_warmup_progress(
        &mut self,
        instance_id: &str,
        detail: WarmupProgressDetail,
    ) -> Result<(), Self::Error> {
        self.runtime.append_runtime_event(
            "warmup",
            Some(instance_id),
            "warmup.progress",
            serde_json::json!({
                "source": detail.source,
                "loaded_bars": detail.loaded_bars,
                "required_bars": detail.required_bars,
                "bar_timestamp": detail.bar_timestamp.to_rfc3339(),
            })
            .to_string(),
        )
    }

    fn record_warmup_ready(
        &mut self,
        instance_id: &str,
        detail: WarmupProgressDetail,
    ) -> Result<(), Self::Error> {
        self.runtime.append_runtime_event(
            "warmup",
            Some(instance_id),
            "warmup.ready",
            serde_json::json!({
                "source": detail.source,
                "loaded_bars": detail.loaded_bars,
                "required_bars": detail.required_bars,
                "bar_timestamp": detail.bar_timestamp.to_rfc3339(),
            })
            .to_string(),
        )
    }

    fn pending_warmup_outcome(
        &self,
        instance_id: &str,
        phase: SignalPhase,
    ) -> Result<Self::Outcome, Self::Error> {
        let instance = self.runtime.instance(instance_id)?;
        let has_position = instance.has_position;
        let bot_id = instance.config.id.clone();
        Ok(ProcessBarOutcome {
            instance_id: bot_id.clone(),
            bot_id,
            symbol: instance.lane_symbol.clone(),
            phase,
            signal: IndicatorSignal::None,
            signal_metadata: None,
            intent: TradeIntent::NoOp,
            strategy_rationale: Some("warmup_pending".to_owned()),
            has_position,
            risk: ProcessBarRisk::Allowed,
        })
    }
}

impl Runtime {
    pub(super) fn record_warmup_failure(
        &mut self,
        instance_id: &str,
        detail: String,
    ) -> Result<(), ServiceError> {
        {
            let instance = self.instance_mut(instance_id)?;
            record_lane_warmup_failure(&mut instance.warmup, detail.clone());
        }

        self.append_runtime_event("warmup", Some(instance_id), "warmup.failed", detail)
    }
}
