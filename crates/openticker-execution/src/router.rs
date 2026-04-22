use crate::{
    AcceptedOrder, ExecutionError, ExecutionRequest, OrderType, order_side_for_intent,
    stable_client_order_id,
};

pub trait ExecutionRouter {
    /// Submits an execution request to the configured destination.
    ///
    /// # Errors
    ///
    /// Returns an [`ExecutionError`] when the request fails validation
    /// or cannot be executed by the router.
    fn submit(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ExecutionError>;
}

#[derive(Debug, Clone, Default)]
pub struct PaperExecutionRouter;

impl ExecutionRouter for PaperExecutionRouter {
    fn submit(&self, request: &ExecutionRequest) -> Result<AcceptedOrder, ExecutionError> {
        if request.quantity <= 0.0 {
            return Err(ExecutionError::InvalidQuantity);
        }
        if request.price <= 0.0 {
            return Err(ExecutionError::InvalidPrice);
        }

        let side = order_side_for_intent(request.intent)?;
        Ok(AcceptedOrder {
            client_order_id: stable_client_order_id(
                &request.instance_id,
                &request.symbol,
                request.timestamp,
                request.intent,
            ),
            side,
            order_type: OrderType::Market,
            price: request.price,
            quantity: request.quantity,
            fee_asset: None,
            fee_amount: None,
            fee_normalized_usd: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use openticker_core::TradeIntent;

    use super::*;
    use crate::OrderSide;

    fn assert_float_eq(left: f64, right: f64) {
        assert!((left - right).abs() < 1e-9, "left={left}, right={right}");
    }

    #[test]
    fn paper_router_validates_order_inputs() {
        let router = PaperExecutionRouter;
        let timestamp = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let request = ExecutionRequest {
            instance_id: "aapl".to_owned(),
            symbol: "AAPL".to_owned(),
            timestamp,
            intent: TradeIntent::OpenLong,
            price: 123.45,
            quantity: 1.0,
        };

        let accepted = router.submit(&request).unwrap();
        assert_eq!(accepted.side, OrderSide::Buy);
        assert_eq!(accepted.order_type, OrderType::Market);
        assert_float_eq(accepted.price, 123.45);
        assert_float_eq(accepted.quantity, 1.0);
        assert!(accepted.fee_asset.is_none());
        assert!(accepted.fee_amount.is_none());
        assert!(accepted.fee_normalized_usd.is_none());

        let invalid = ExecutionRequest {
            quantity: 0.0,
            ..request
        };
        assert!(matches!(
            router.submit(&invalid),
            Err(ExecutionError::InvalidQuantity)
        ));
    }
}
