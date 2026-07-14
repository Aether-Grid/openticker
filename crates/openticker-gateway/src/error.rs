use openticker_connectors::{ConnectionState, ConnectorError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("connector registry lock poisoned")]
    LockPoisoned,
    #[error("account `{account_id}` has unsupported connector kind `{kind}`")]
    UnsupportedConnectorKind { account_id: String, kind: String },
    #[error("connector account `{account_id}` is not registered")]
    UnknownAccount { account_id: String },
    #[error("connector account `{account_id}` is not ready (`{state:?}`): {reason}")]
    ConnectorNotReady {
        account_id: String,
        state: ConnectionState,
        reason: String,
    },
    #[error(transparent)]
    Connector(ConnectorError),
}

impl From<ConnectorError> for GatewayError {
    fn from(error: ConnectorError) -> Self {
        match error {
            ConnectorError::UnknownAccount { account_id } => Self::UnknownAccount { account_id },
            other => Self::Connector(other),
        }
    }
}
