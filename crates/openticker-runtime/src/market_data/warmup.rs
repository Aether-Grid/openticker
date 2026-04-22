use super::warmup_engine::RuntimeLaneWarmupEngine;
use crate::{OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, SignalPhase};
use openticker_lane::{
    attempt_lane_warmup_backfill, process_pending_warmup_bar as process_lane_pending_warmup_bar,
};

impl Runtime {
    pub(crate) fn run_startup_warmup(&mut self) {
        let instance_ids = self
            .state
            .lanes
            .iter()
            .filter_map(|(instance_id, instance)| {
                if instance.config.enabled {
                    Some(instance_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for instance_id in instance_ids {
            if let Err(error) = self.attempt_instance_warmup_backfill(&instance_id, "startup") {
                let _ = self
                    .record_warmup_failure(&instance_id, format!("startup warmup failed: {error}"));
            }
        }
    }

    pub(crate) fn attempt_instance_warmup_backfill(
        &mut self,
        instance_id: &str,
        source: &'static str,
    ) -> Result<(), ServiceError> {
        attempt_lane_warmup_backfill(
            &mut RuntimeLaneWarmupEngine { runtime: self },
            instance_id,
            source,
        )
    }

    pub(crate) fn process_pending_warmup_bar(
        &mut self,
        instance_id: &str,
        bar: &OhlcvBar,
        phase: SignalPhase,
    ) -> Result<Option<ProcessBarOutcome>, ServiceError> {
        process_lane_pending_warmup_bar(
            &mut RuntimeLaneWarmupEngine { runtime: self },
            instance_id,
            bar,
            phase,
        )
    }
}

#[cfg(test)]
#[path = "warmup_tests.rs"]
mod tests;
