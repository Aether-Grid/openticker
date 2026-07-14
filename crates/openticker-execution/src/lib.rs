mod error;
mod intent;
mod router;
mod sizing;
mod types;

#[cfg(kani)]
mod proofs;

pub use error::ExecutionError;
pub use intent::{order_side_for_intent, stable_client_order_id};
pub use router::{ExecutionRouter, PaperExecutionRouter};
pub use sizing::resolve_order_quantity_with_constraints;
pub use types::{
    AcceptedOrder, ExecutionRequest, OrderLedgerOutcome, OrderQuantityResolution, OrderSide,
    OrderType,
};
