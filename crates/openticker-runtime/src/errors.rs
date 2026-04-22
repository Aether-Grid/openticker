use crate::LaneRuntimeState;
use openticker_connectors::ConnectionState;
use openticker_data::DataError;
use openticker_execution::ExecutionError;
use openticker_instance::InstanceError;
use openticker_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("instance `{0}` does not exist")]
    InstanceNotFound(String),
    #[error("instance `{0}` is disabled")]
    InstanceDisabled(String),
    #[error("instance `{instance_id}` cannot trade until reconciliation succeeds: {reason}")]
    ReconciliationRequired { instance_id: String, reason: String },
    #[error("kill switch is active")]
    KillSwitchEnabled,
    #[error("cannot {action} instance `{instance_id}` while state is `{state:?}`")]
    InvalidTransition {
        instance_id: String,
        state: LaneRuntimeState,
        action: &'static str,
    },
    #[error(
        "trade symbol mismatch for instance `{instance_id}`: expected `{expected}`, got `{received}`"
    )]
    TradeSymbolMismatch {
        instance_id: String,
        expected: String,
        received: String,
    },
    #[error("instance `{instance_id}` requires a symbol selector to {action}")]
    SymbolSelectionRequired {
        instance_id: String,
        action: &'static str,
    },
    #[error("instance `{instance_id}` does not configure symbol `{symbol}`")]
    SymbolNotConfigured { instance_id: String, symbol: String },
    #[error(
        "connector for instance `{instance_id}` account `{account_id}` is not ready (`{state:?}`): {reason}"
    )]
    ConnectorNotReady {
        instance_id: String,
        account_id: String,
        state: ConnectionState,
        reason: String,
    },
    #[error(
        "data connector for instance `{instance_id}` account `{account_id}` failed to fetch latest bar: {reason}"
    )]
    DataConnectorUnavailable {
        instance_id: String,
        account_id: String,
        reason: String,
    },
    #[error(
        "execution connector for instance `{instance_id}` account `{account_id}` failed to submit order: {reason}"
    )]
    ExecutionConnectorUnavailable {
        instance_id: String,
        account_id: String,
        reason: String,
    },
    #[error("invalid service configuration: {0}")]
    InvalidConfiguration(String),
    #[error("data error: {0}")]
    Data(#[from] DataError),
    #[error("execution error: {0}")]
    Execution(#[from] ExecutionError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<InstanceError> for ServiceError {
    fn from(error: InstanceError) -> Self {
        match error {
            InstanceError::InvalidConfiguration(message) => Self::InvalidConfiguration(message),
        }
    }
}
