use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("{0} cannot be empty")]
    InvalidIdentifier(&'static str),
    #[error("unsupported timeframe `{0}`")]
    InvalidTimeframe(String),
}
