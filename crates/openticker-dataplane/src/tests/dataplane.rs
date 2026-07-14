use super::{bar, key};
use crate::{DataPlane, StreamSource, StreamSpec, StreamUpdateSource};
use openticker_core::Timeframe;

#[test]
fn dataplane_only_returns_due_streams() {
    let data_plane = DataPlane::new([
        StreamSpec {
            key: key("alpaca-paper", "AAPL", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 1_000,
            close_poll_retry_ms: None,
            close_poll_grace_ms: None,
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl".to_owned())],
        },
        StreamSpec {
            key: key("alpaca-paper", "SPY", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 5_000,
            close_poll_retry_ms: None,
            close_poll_grace_ms: None,
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
fn dataplane_retries_inside_close_freshness_window() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 60_000,
        close_poll_retry_ms: Some(2_000),
        close_poll_grace_ms: Some(30_000),
        preview_enabled: false,
        sources: vec![StreamSource::Instance("aapl".to_owned())],
    }]);

    data_plane
        .record_fetched_bar(&stream_key, 0, bar(0, 100.0))
        .unwrap();
    data_plane
        .record_manual_poll_attempt(&stream_key, 59_500)
        .unwrap();

    assert!(data_plane.take_due_streams(60_500).is_empty());
    assert_eq!(
        data_plane.take_due_streams(61_500),
        vec![stream_key.clone()]
    );

    data_plane
        .record_fetched_bar(&stream_key, 61_500, bar(1, 101.0))
        .unwrap();
    assert!(data_plane.take_due_streams(63_500).is_empty());
}

#[test]
fn dataplane_does_not_fast_retry_after_stale_deadline() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 60_000,
        close_poll_retry_ms: Some(2_000),
        close_poll_grace_ms: Some(30_000),
        preview_enabled: false,
        sources: vec![StreamSource::Instance("aapl".to_owned())],
    }]);

    data_plane
        .record_fetched_bar(&stream_key, 0, bar(0, 100.0))
        .unwrap();
    data_plane
        .record_manual_poll_attempt(&stream_key, 89_000)
        .unwrap();

    assert!(data_plane.take_due_streams(91_500).is_empty());
}

#[test]
fn snapshot_includes_attached_instances_and_staleness() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 1_000,
        close_poll_retry_ms: None,
        close_poll_grace_ms: None,
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
    assert_eq!(snapshot[0].transport_staleness_ms, Some(750));
    assert_eq!(snapshot[0].confirmed_bar_close_ms, Some(120_000));
    assert_eq!(snapshot[0].confirmed_bar_staleness_ms, Some(0));
    assert_eq!(snapshot[0].confirmed_bar_stale_deadline_ms, None);
    assert_eq!(
        snapshot[0].attached_instances,
        vec!["aapl-confirmation".to_owned(), "aapl-primary".to_owned()]
    );
    assert_eq!(snapshot[0].sparkline, vec![100.0]);
}

#[test]
fn duplicate_confirmed_fetch_does_not_advance_confirmed_source_or_freshness() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 1_000,
        close_poll_retry_ms: Some(2_000),
        close_poll_grace_ms: Some(30_000),
        preview_enabled: false,
        sources: vec![StreamSource::Instance("aapl".to_owned())],
    }]);

    assert!(
        data_plane
            .record_fetched_bar(&stream_key, 60_500, bar(1, 100.0))
            .unwrap()
    );
    let first = data_plane.snapshot_streams(61_000, 30).remove(0);
    assert_eq!(
        first.last_confirmed_update_source,
        Some(StreamUpdateSource::Poll)
    );
    assert_eq!(first.confirmed_bar_close_ms, Some(120_000));
    assert_eq!(first.confirmed_bar_staleness_ms, Some(0));

    assert!(
        !data_plane
            .record_fetched_bar_from_source(
                &stream_key,
                62_000,
                bar(1, 101.0),
                StreamUpdateSource::PreviewStream,
            )
            .unwrap()
    );

    let duplicate = data_plane.snapshot_streams(62_500, 30).remove(0);
    assert_eq!(duplicate.transport_staleness_ms, Some(500));
    assert_eq!(
        duplicate.last_confirmed_update_source,
        Some(StreamUpdateSource::Poll)
    );
    assert_eq!(
        duplicate.confirmed_bar_close_ms,
        first.confirmed_bar_close_ms
    );
    assert_eq!(duplicate.confirmed_bar_staleness_ms, Some(0));
    assert!((duplicate.latest_bar.unwrap().close - 100.0).abs() < f64::EPSILON);
}

#[test]
fn replace_streams_preserves_existing_buffer_for_surviving_stream() {
    let stream_key = key("alpaca-paper", "AAPL", Timeframe::M1);
    let data_plane = DataPlane::new([StreamSpec {
        key: stream_key.clone(),
        retention: 500,
        polling_interval_ms: 1_000,
        close_poll_retry_ms: None,
        close_poll_grace_ms: None,
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
            close_poll_retry_ms: None,
            close_poll_grace_ms: None,
            preview_enabled: false,
            sources: vec![StreamSource::Instance("aapl".to_owned())],
        },
        StreamSpec {
            key: key("alpaca-paper", "SPY", Timeframe::M1),
            retention: 500,
            polling_interval_ms: 1_000,
            close_poll_retry_ms: None,
            close_poll_grace_ms: None,
            preview_enabled: false,
            sources: vec![StreamSource::Watchlist],
        },
    ]);

    let bars = data_plane.snapshot_bars(&stream_key, 10).unwrap();
    assert_eq!(bars.len(), 1);
    assert!((bars[0].close - 100.0).abs() < f64::EPSILON);
}
