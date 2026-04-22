mod capabilities;
mod error;
mod helpers;
mod stub;
mod traits;
mod types;

pub use capabilities::{connector_matrix, descriptor_for};
pub use error::ConnectorError;
pub use stub::StubConnector;
pub use traits::{
    ConnectorClient, ConnectorExecution, ConnectorHealth, ConnectorMarketData,
    ConnectorMarketStream, ConnectorPrivateStream, ConnectorReconcile, ConnectorRuntimeControl,
    ConnectorSymbolConstraintsLookup,
};
pub use types::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot,
    ConnectorAccountStatus, ConnectorDescriptor, ConnectorKind, ConnectorMarketStreamSubscription,
    ConnectorOpenOrder, ConnectorPosition, ConnectorPreviewStreamCommand,
    ConnectorPreviewStreamEvent, ConnectorPreviewStreamSession, ConnectorPrivateAccountEvent,
    ConnectorPrivateBalance, ConnectorPrivateStreamEvent, ConnectorResiliencePolicy,
    ConnectorResilienceState, ConnectorRole, ConnectorStatus, ConnectorSymbolConstraints,
    PreviewStreamConnectionState,
};

pub(crate) use helpers::{
    default_blocking_http_client, deterministic_remote_client_order_id, format_decimal_quantity,
    resolve_secret_env_value, run_in_blocking_thread, unix_now_ms,
};

mod connectors;
pub use connectors::{alpaca, binance};

mod registry;
pub use registry::ConnectorRegistry;

#[cfg(test)]
mod tests;
