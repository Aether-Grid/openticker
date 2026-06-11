use crate::types::{ConnectionState, ConnectorKind};
use openticker_core::ExecutionMode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector account `{account_id}` is not registered")]
    UnknownAccount { account_id: String },
    #[error("connector account `{account_id}` is configured more than once")]
    DuplicateAccountId { account_id: String },
    #[error("connector `{kind:?}` requires demo mode for paper operation")]
    DemoModeRequired { kind: ConnectorKind },
    #[error("connector `{kind:?}` does not support `{mode:?}` mode")]
    UnsupportedMode {
        kind: ConnectorKind,
        mode: ExecutionMode,
    },
    #[error("connector `{kind:?}` requires credential reference `{field}`")]
    MissingCredentialReference {
        kind: ConnectorKind,
        field: &'static str,
    },
    #[error("connector `{kind:?}` is missing environment value for `{env_var}`")]
    MissingCredentialValue {
        kind: ConnectorKind,
        env_var: String,
    },
    #[error("connector `{kind:?}` failed to submit order: {detail}")]
    OrderSubmission { kind: ConnectorKind, detail: String },
    #[error("connector `{kind:?}` failed to decode market stream payload: {detail}")]
    StreamDecode { kind: ConnectorKind, detail: String },
    #[error("connector `{kind:?}` failed to start preview stream: {detail}")]
    PreviewStream { kind: ConnectorKind, detail: String },
    #[error("connector `{kind:?}` cannot fetch remote snapshot: {detail}")]
    RemoteSnapshot { kind: ConnectorKind, detail: String },
    #[error("connector `{kind:?}` is rate limited: {detail}")]
    RateLimited {
        kind: ConnectorKind,
        /// Time until the provider expects requests to resume, when reported.
        retry_after_ms: Option<u64>,
        detail: String,
    },
    #[error("connector `{kind:?}` is `{state:?}` and cannot serve reconciliation state")]
    Unavailable {
        kind: ConnectorKind,
        state: ConnectionState,
    },
}
