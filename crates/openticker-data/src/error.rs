use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("trade price must be greater than zero")]
    InvalidPrice,
    #[error("trade quantity cannot be negative")]
    InvalidQuantity,
}
