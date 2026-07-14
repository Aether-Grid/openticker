use crate::POSITION_QUANTITY_TOLERANCE;
use openticker_storage::PositionRecord;

#[derive(Debug, Clone)]
pub struct LatestLanePosition {
    pub lane_id: String,
    pub position: Option<PositionRecord>,
}

#[must_use]
pub fn latest_authoritative_position(
    symbol: &str,
    positions: &[PositionRecord],
) -> Option<PositionRecord> {
    positions.iter().rev().find_map(|position| {
        (position_record_is_authoritative(position) && position.symbol.as_deref() == Some(symbol))
            .then_some(position.clone())
    })
}

pub(crate) fn latest_position_for_lane<'a>(
    latest_positions: &'a [LatestLanePosition],
    lane_id: &str,
) -> Option<&'a PositionRecord> {
    latest_positions
        .iter()
        .find(|latest| latest.lane_id == lane_id)
        .and_then(|latest| latest.position.as_ref())
}

pub(crate) fn authoritative_position_quantity(position: &PositionRecord) -> f64 {
    if !position_record_is_authoritative(position) {
        return 0.0;
    }

    if position.quantity > POSITION_QUANTITY_TOLERANCE {
        position.quantity.max(0.0)
    } else if position.has_position {
        1.0
    } else {
        0.0
    }
}

pub(crate) fn position_record_indicates_open(position: &PositionRecord) -> bool {
    authoritative_position_quantity(position) > POSITION_QUANTITY_TOLERANCE
        || (position_record_is_authoritative(position) && position.has_position)
}

fn position_record_is_authoritative(position: &PositionRecord) -> bool {
    !position_reason_is_close_requested(position.reason.as_str())
        && !position_reason_is_reconciliation_sync(position.reason.as_str())
}

fn position_reason_is_close_requested(reason: &str) -> bool {
    reason == "close_requested"
}

pub(crate) fn position_reason_is_reconciliation_sync(reason: &str) -> bool {
    reason.ends_with("_reconciliation_sync")
}
