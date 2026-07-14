mod balances;
mod exceptions;
mod exposure;
mod lanes;
mod ledger_sync;
mod orders;
mod positions;
mod reconciliation;
mod rejections;
mod symbols;

pub use balances::live_balance_from_snapshot;
pub use exceptions::{managed_position_deficit_exceptions, unmapped_managed_open_order_exceptions};
pub use exposure::{
    AccountSymbolExposure, ConnectorPositionOwner, account_symbol_exposure,
    connector_position_owner, position_quantity_for_symbol,
};
pub use lanes::{AccountRiskSnapshot, PortfolioLaneView, account_risk_snapshot};
pub use ledger_sync::{
    AccountLedgerRefreshState, LedgerRooms, account_ledger_refresh_state,
    apply_account_ledger_refresh_state, lane_open_notionals, ledger_rooms, ledger_snapshot,
    sync_account_ledger_from_lanes,
};
pub use orders::{
    ClassifiedRemoteOpenOrders, LocalOpenOrderIdentity, ManagedRemoteOpenOrder,
    classify_remote_open_orders, local_open_order_ids, open_orders_for_symbol,
};
pub use positions::{LatestLanePosition, latest_authoritative_position};
pub use reconciliation::{
    ReconciliationAssessment, ReconciliationAssessmentSummary, ReconciliationPositions,
    build_reconciliation_assessment, reconciliation_assessment_summary, reconciliation_differences,
};
pub use rejections::{
    LedgerRejectionEventPayload, LedgerRejectionPayload, account_ledger_rejection_payload,
    bot_ledger_rejection_payload, dust_ledger_rejection_payload, ledger_rejection_event_payload,
};

const POSITION_QUANTITY_TOLERANCE: f64 = 1e-9;

#[cfg(test)]
mod tests;
