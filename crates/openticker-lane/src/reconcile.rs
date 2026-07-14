use openticker_connectors::ConnectorAccountSnapshot;

#[derive(Debug)]
pub struct ConnectorSnapshotOutcome {
    pub snapshot: Option<ConnectorAccountSnapshot>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Default)]
pub struct ReconciliationSyncOutcome {
    pub position_synced: bool,
    pub stale_local_open_orders_closed_count: usize,
    pub remote_open_orders_backfilled_count: usize,
}
