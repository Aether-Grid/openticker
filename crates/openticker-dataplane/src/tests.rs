use super::*;
use chrono::{TimeZone, Utc};
use openticker_core::{OhlcvBar, Timeframe};

#[test]
fn duplicate_stream_requests_share_one_registry_entry() {
    let key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let registry = StreamRegistry::from_specs([
        StreamSpec {
            key: key.clone(),
            retention: 500,
            polling_interval_ms: 5_000,
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl-fast".to_owned())],
        },
        StreamSpec {
            key,
            retention: 700,
            polling_interval_ms: 1_000,
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
    assert_eq!(
        specs[0].sources,
        vec![
            StreamSource::Watchlist,
            StreamSource::Instance("aapl-fast".to_owned()),
            StreamSource::Instance("aapl-slow".to_owned()),
        ]
    );
}

#[test]
fn older_bar_is_ignored_without_mutating_buffer() {
    let mut buffer = StreamBuffer::new(3);
    assert!(buffer.push_if_newer(bar(1, 100.0)));
    assert!(buffer.push_if_newer(bar(3, 102.0)));

    let before = buffer.snapshot(10);
    assert!(!buffer.push_if_newer(bar(2, 101.0)));
    let after = buffer.snapshot(10);
    assert_eq!(before, after);

    let closes = buffer
        .snapshot(10)
        .into_iter()
        .map(|bar| bar.close)
        .collect::<Vec<_>>();
    assert_eq!(closes, vec![100.0, 102.0]);
}

#[test]
fn dataplane_only_returns_due_streams() {
    let data_plane = DataPlane::new([
        StreamSpec {
            key: key("alpaca-paper", "AAPL", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 1_000,
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl".to_owned())],
        },
        StreamSpec {
            key: key("alpaca-paper", "SPY", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 5_000,
            preview_enabled: false,
            sources: vec![StreamSource::Watchlist],
        },
    ]);

    let due = data_plane.take_due_streams(1_000);
    assert_eq!(due.len(), 2);

    let due = data_plane.take_due_streams(1_500);
    assert!(due.is_empty());

    let due = data_plane.take_due_streams(2_100);
    assert_eq!(due, vec![key("alpaca-paper", "AAPL", Timeframe::M1)]);
}

#[test]
fn snapshot_includes_attached_instances_and_staleness() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 1_000,
        preview_enabled: false,
        sources: vec![
            StreamSource::Watchlist,
            StreamSource::Instance("aapl-primary".to_owned()),
            StreamSource::Instance("aapl-confirmation".to_owned()),
        ],
    }]);

    let _ = data_plane.take_due_streams(1_000);
    data_plane
        .record_fetched_bar(&stream_key, 1_250, bar(1, 100.0))
        .unwrap();

    let snapshot = data_plane.snapshot_streams(2_000, 30);
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].staleness_ms, Some(750));
    assert_eq!(
        snapshot[0].attached_instances,
        vec!["aapl-confirmation".to_owned(), "aapl-primary".to_owned()]
    );
    assert_eq!(snapshot[0].sparkline, vec![100.0]);
}

#[test]
fn replace_streams_preserves_existing_buffer_for_surviving_stream() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 1_000,
        preview_enabled: false,
        sources: vec![StreamSource::Instance("aapl".to_owned())],
    }]);

    let _ = data_plane.take_due_streams(1_000);
    data_plane
        .record_fetched_bar(&stream_key, 1_000, bar(1, 100.0))
        .unwrap();

    data_plane.replace_streams([
        StreamSpec {
            key: stream_key.clone(),
            retention: 10,
            polling_interval_ms: 2_000,
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl".to_owned())],
        },
        StreamSpec {
            key: key("alpaca-paper", "SPY", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 1_000,
            preview_enabled: false,
            sources: vec![StreamSource::Watchlist],
        },
    ]);

    let bars = data_plane.snapshot_bars(&stream_key, 10).unwrap();
    assert_eq!(bars.len(), 1);
    assert!((bars[0].close - 100.0).abs() < f64::EPSILON);
}

fn key(account_id: &str, symbol: &str, timeframe: Timeframe) -> StreamKey {
    StreamKey {
        account_id: account_id.to_owned(),
        symbol: symbol.to_owned(),
        timeframe,
    }
}

fn bar(minute: i64, close: f64) -> OhlcvBar {
    OhlcvBar {
        timestamp: Utc.timestamp_opt(minute * 60, 0).single().unwrap(),
        open: close,
        high: close,
        low: close,
        close,
        volume: 1.0,
    }
}
