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
    PREVIEW_STREAM_COMMAND_CAPACITY, PREVIEW_STREAM_EVENT_CAPACITY, PreviewStreamConnectionState,
};

pub(crate) use helpers::{
    default_blocking_http_client, deterministic_remote_client_order_id, format_decimal_quantity,
    rate_limit_error, resolve_secret_env_value, retry_after_header, run_in_blocking_thread,
    sanitize_symbol_for_error, unix_now_ms,
};

mod connectors;
pub use connectors::{alpaca, binance};

mod registry;
pub use registry::{ConnectorClientHandle, ConnectorRegistry};

#[cfg(test)]
mod tests;
