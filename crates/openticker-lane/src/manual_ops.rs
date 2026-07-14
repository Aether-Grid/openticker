use openticker_core::{OhlcvBar, TradeIntent};

#[derive(Debug, Clone)]
pub struct ManualCloseContext {
    pub bot_id: String,
    pub account_id: String,
    pub reconciliation_remote_snapshot: bool,
    pub has_local_position: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualCloseSignalRisk {
    Allowed,
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualCloseSignalOutcome {
    pub intent: TradeIntent,
    pub risk: ManualCloseSignalRisk,
}

#[derive(Debug, Clone)]
pub enum ManualCloseOutcome {
    AlreadyFlat,
    Processed {
        intent: TradeIntent,
        risk: ManualCloseSignalRisk,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

#[allow(clippy::missing_errors_doc)]
pub trait LaneManualOpsEngine {
    type Error;

    fn manual_close_context(&self, instance_id: &str) -> Result<ManualCloseContext, Self::Error>;
    fn sync_remote_position_for_manual_close(
        &mut self,
        instance_id: &str,
        account_id: &str,
    ) -> Result<bool, Self::Error>;
    fn fetch_latest_bar_for_manual_close(
        &mut self,
        instance_id: &str,
    ) -> Result<OhlcvBar, Self::Error>;
    fn process_manual_close_signal(
        &mut self,
        instance_id: &str,
        price: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<ManualCloseSignalOutcome, Self::Error>;
}

/// Runs the shared manual-close workflow for a lane through runtime-provided
/// connector and journal ports.
///
/// # Errors
///
/// Propagates any workflow error returned by the engine implementation.
pub fn close_lane_position<E: LaneManualOpsEngine>(
    engine: &mut E,
    instance_id: &str,
) -> Result<ManualCloseOutcome, E::Error> {
    let context = engine.manual_close_context(instance_id)?;
    let has_position = if context.reconciliation_remote_snapshot {
        engine.sync_remote_position_for_manual_close(instance_id, &context.account_id)?
    } else {
        context.has_local_position
    };

    if !has_position {
        return Ok(ManualCloseOutcome::AlreadyFlat);
    }

    let latest_bar = engine.fetch_latest_bar_for_manual_close(instance_id)?;
    let signal_outcome =
        engine.process_manual_close_signal(instance_id, latest_bar.close, latest_bar.timestamp)?;

    Ok(ManualCloseOutcome::Processed {
        intent: signal_outcome.intent,
        risk: signal_outcome.risk,
        price: latest_bar.close,
        timestamp: latest_bar.timestamp,
    })
}
