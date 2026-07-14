use super::key;
use crate::{StreamRegistry, StreamSource, StreamSpec};
use openticker_core::Timeframe;

#[test]
fn duplicate_stream_requests_share_one_registry_entry() {
    let key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let registry = StreamRegistry::from_specs([
        StreamSpec {
            key: key.clone(),
            retention: 500,
            polling_interval_ms: 5_000,
            close_poll_retry_ms: Some(4_000),
            close_poll_grace_ms: Some(60_000),
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl-fast".to_owned())],
        },
        StreamSpec {
            key,
            retention: 700,
            polling_interval_ms: 1_000,
            close_poll_retry_ms: Some(2_000),
            close_poll_grace_ms: Some(30_000),
            preview_enabled: false,
            sources: vec![
                StreamSource::Watchlist,
                StreamSource::Instance("aapl-slow".to_owned()),
            ],
        },
    ]);

    let specs = registry.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].retention, 700);
    assert_eq!(specs[0].polling_interval_ms, 1_000);
    assert_eq!(specs[0].close_poll_retry_ms, Some(2_000));
    assert_eq!(specs[0].close_poll_grace_ms, Some(30_000));
    assert_eq!(
        specs[0].sources,
        vec![
            StreamSource::Watchlist,
            StreamSource::Instance("aapl-fast".to_owned()),
            StreamSource::Instance("aapl-slow".to_owned()),
        ]
    );
}
