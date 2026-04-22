use crate::connector_gateway::ConnectorGatewayRead;
use crate::{ConnectorSnapshotOutcome, ServiceError};
use openticker_gateway::GatewayError;

impl ConnectorGatewayRead<'_> {
    pub(crate) fn fetch_account_snapshot_outcome(
        &self,
        lane_id: &str,
        account_id: &str,
    ) -> Result<ConnectorSnapshotOutcome, ServiceError> {
        let connector_kind = self.account_kind(lane_id, account_id)?;
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.account_snapshot",
            account_id,
            &connector_kind,
            "fetch_account_snapshot",
        );

        provider.record_stage(
            "requested",
            "requesting remote account snapshot",
            serde_json::json!({}),
        )?;

        match self.gateway().fetch_account_snapshot_unchecked(account_id) {
            Ok(snapshot) => {
                provider.record_stage(
                    "succeeded",
                    format!(
                        "snapshot received with {} open orders, {} positions, {} balances",
                        snapshot.open_orders.len(),
                        snapshot.positions.len(),
                        snapshot.balances.len(),
                    ),
                    serde_json::json!({
                        "response": {
                            "open_orders_count": snapshot.open_orders.len(),
                            "positions_count": snapshot.positions.len(),
                            "balances_count": snapshot.balances.len(),
                        },
                    }),
                )?;
                Ok(ConnectorSnapshotOutcome {
                    snapshot: Some(snapshot),
                    unavailable_reason: None,
                })
            }
            Err(GatewayError::UnknownAccount { .. }) => Err(ServiceError::InvalidConfiguration(
                format!("missing connector status for account `{account_id}`"),
            )),
            Err(GatewayError::LockPoisoned) => Err(ServiceError::InvalidConfiguration(
                "connector registry lock poisoned".to_owned(),
            )),
            Err(error) => {
                let _ = provider.record_stage(
                    "failed",
                    "remote account snapshot unavailable",
                    serde_json::json!({
                        "error": error.to_string(),
                    }),
                );
                Ok(ConnectorSnapshotOutcome {
                    snapshot: None,
                    unavailable_reason: Some(format!("connector_snapshot_unavailable({error})")),
                })
            }
        }
    }
}
