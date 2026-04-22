use crate::types::{ConnectorDescriptor, ConnectorKind, ConnectorResiliencePolicy, ConnectorRole};

#[must_use]
pub fn descriptor_for(kind: ConnectorKind) -> ConnectorDescriptor {
    match kind {
        ConnectorKind::Alpaca => ConnectorDescriptor {
            kind,
            role: ConnectorRole::Dual,
            supports_paper: true,
            supports_live: true,
            supports_demo: false,
            resilience: ConnectorResiliencePolicy {
                reconnect_base_backoff_ms: 500,
                reconnect_max_backoff_ms: 30_000,
                reconnect_jitter_bps: 2_000,
                requires_ping_pong_keepalive: true,
                max_connection_lifetime_secs: None,
                max_messages_per_second: None,
                single_connection_per_api_key: false,
            },
        },
        ConnectorKind::Binance => ConnectorDescriptor {
            kind,
            role: ConnectorRole::Dual,
            supports_paper: false,
            supports_live: true,
            supports_demo: true,
            resilience: ConnectorResiliencePolicy {
                reconnect_base_backoff_ms: 250,
                reconnect_max_backoff_ms: 30_000,
                reconnect_jitter_bps: 2_000,
                requires_ping_pong_keepalive: true,
                max_connection_lifetime_secs: Some(86_400),
                max_messages_per_second: Some(5),
                single_connection_per_api_key: false,
            },
        },
    }
}

#[must_use]
pub fn connector_matrix() -> Vec<ConnectorDescriptor> {
    vec![
        descriptor_for(ConnectorKind::Alpaca),
        descriptor_for(ConnectorKind::Binance),
    ]
}
