//! Static connector capability matrix consulted by account and instance validation.

use openticker_core::MarketType;

#[derive(Debug, Clone, Copy)]
pub(super) struct ConnectorCapabilities {
    pub(super) roles: ConnectorRoleCapabilities,
    pub(super) markets: ConnectorMarketCapabilities,
    pub(super) secrets: ConnectorSecretRequirements,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ConnectorRoleCapabilities {
    pub(super) data: bool,
    pub(super) execution: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ConnectorMarketCapabilities {
    pub(super) equities: bool,
    pub(super) crypto: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ConnectorSecretRequirements {
    pub(super) api_key: bool,
    pub(super) api_secret: bool,
    pub(super) passphrase: bool,
}

pub(super) fn connector_capabilities(kind: &str) -> Option<ConnectorCapabilities> {
    match kind {
        "alpaca" => Some(ConnectorCapabilities {
            roles: ConnectorRoleCapabilities {
                data: true,
                execution: true,
            },
            markets: ConnectorMarketCapabilities {
                equities: true,
                crypto: false,
            },
            secrets: ConnectorSecretRequirements {
                api_key: true,
                api_secret: true,
                passphrase: false,
            },
        }),
        "binance" => Some(ConnectorCapabilities {
            roles: ConnectorRoleCapabilities {
                data: true,
                execution: true,
            },
            markets: ConnectorMarketCapabilities {
                equities: false,
                crypto: true,
            },
            secrets: ConnectorSecretRequirements {
                api_key: true,
                api_secret: true,
                passphrase: false,
            },
        }),
        _ => None,
    }
}

pub(super) fn connector_supports_market(caps: ConnectorCapabilities, market: MarketType) -> bool {
    match market {
        MarketType::Equities => caps.markets.equities,
        MarketType::Crypto => caps.markets.crypto,
    }
}

/// Reports whether a data connector can emit *preview* (in-progress, unconfirmed)
/// market-stream bars, which `signal_mode = "intrabar"` requires.
///
/// Only `binance` qualifies today: it is the sole connector that implements a
/// real preview stream. See its `start_preview_stream_session` in
/// `openticker-connectors/src/connectors/binance.rs`, which opens a websocket
/// feed of partial klines. Other connectors (e.g. `alpaca`) implement only the
/// confirmed-bar path, so intrabar mode would silently never fire.
///
/// This is kept as a single documented match (rather than a general capability
/// framework) because preview support is currently a binary, connector-specific
/// fact. When a second connector gains a preview stream, add its kind here and
/// keep this comment in sync with the connector that implements it.
pub(super) fn connector_supports_preview_market_stream(kind: &str) -> bool {
    matches!(kind, "binance")
}

pub(super) fn market_type_label(market: MarketType) -> &'static str {
    match market {
        MarketType::Equities => "equities",
        MarketType::Crypto => "crypto",
    }
}
