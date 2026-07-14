//! Validation tests for global data-plane settings.

use super::support::{
    connector_validation_bundle, default_validation_account, default_validation_instance,
};
use crate::{ConfigError, DataPlaneConfig, DataPlaneWatchlistEntry};
use openticker_core::Timeframe;

#[test]
fn accepts_data_plane_watchlist_using_defaults() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle.global.data_plane = DataPlaneConfig {
        default_polling_interval_ms: 5_000,
        default_retention: 500,
        watchlist: vec![DataPlaneWatchlistEntry {
            account: "alpaca-paper".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: None,
            retention: None,
        }],
    };

    assert!(bundle.validate().is_ok());
}

#[test]
fn rejects_data_plane_watchlist_with_unknown_account() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle
        .global
        .data_plane
        .watchlist
        .push(DataPlaneWatchlistEntry {
            account: "missing-account".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: None,
            retention: None,
        });

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("unknown account")
    ));
}

#[test]
fn rejects_data_plane_watchlist_with_invalid_interval_or_retention() {
    let mut bundle =
        connector_validation_bundle(default_validation_account(), default_validation_instance());
    bundle
        .global
        .data_plane
        .watchlist
        .push(DataPlaneWatchlistEntry {
            account: "alpaca-paper".to_owned(),
            symbol: "SPY".to_owned(),
            timeframe: Timeframe::M1,
            polling_interval_ms: Some(500),
            retention: Some(5),
        });

    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("polling_interval_ms")
    ));

    bundle.global.data_plane.watchlist[0].polling_interval_ms = Some(1_000);
    assert!(matches!(
        bundle.validate(),
        Err(ConfigError::Validation { message }) if message.contains("retention")
    ));
}
