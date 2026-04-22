use crate::connector_gateway::ConnectorGatewayRead;
use crate::{AcceptedOrder, ExecutionRequest, ServiceError, trade_intent_label};

impl ConnectorGatewayRead<'_> {
    pub(crate) fn submit_order(
        &self,
        lane_id: &str,
        account_id: &str,
        connector_kind: &str,
        execution_request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, ServiceError> {
        let repo = self.repo();
        let provider = repo.provider_operation(
            lane_id,
            "provider.order_submission",
            account_id,
            connector_kind,
            "submit_order",
        );
        let request = execution_request.clone();

        provider.record_call(
            "requested",
            format!(
                "submitting {} {} @ {:.4}",
                trade_intent_label(execution_request.intent),
                execution_request.symbol,
                execution_request.price,
            ),
            serde_json::json!({
                "request": request.clone(),
            }),
            || self.gateway().submit_order(account_id, execution_request),
            |accepted_order| {
                (
                    "succeeded",
                    format!(
                        "{} accepted {} {:.4} @ {:.4}",
                        connector_kind,
                        execution_request.symbol,
                        accepted_order.quantity,
                        accepted_order.price,
                    ),
                    serde_json::json!({
                        "request": request.clone(),
                        "response": accepted_order.clone(),
                    }),
                )
            },
            |error| {
                (
                    format!("order submission failed for {}", execution_request.symbol),
                    serde_json::json!({
                        "request": request.clone(),
                        "error": error.to_string(),
                    }),
                )
            },
            |error| Self::map_execution_error(lane_id, account_id, error),
        )
    }
}
