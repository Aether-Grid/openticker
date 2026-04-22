use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("intent is not executable")]
    NonExecutableIntent,
    #[error("order quantity must be greater than zero")]
    InvalidQuantity,
    #[error("order price must be greater than zero")]
    InvalidPrice,
}
