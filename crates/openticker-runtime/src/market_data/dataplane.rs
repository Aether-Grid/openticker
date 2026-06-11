use crate::{
    ConnectorRegistry, OhlcvBar, Runtime, ServiceError, StreamKey, StreamRegistry, StreamSource,
    StreamSpec,
};
use openticker_config::SignalMode;
use openticker_connectors::ConnectorMarketStreamSubscription;
use openticker_core::ExecutionMode;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const CLOSE_POLL_RETRY_MS: u64 = 2_000;
const MAX_CONFIRMED_POLL_INTERVAL_MS: u64 = 60_000;

/// Derives the steady-state confirmed-bar polling interval for an instance
/// stream.
///
/// Confirmed bars only change once per timeframe and the close-window
/// fast-poll path (every [`CLOSE_POLL_RETRY_MS`] within the post-close grace
/// window) picks up freshly closed bars within seconds, so the steady-state
/// cadence scales with the timeframe instead of polling the connector at the
/// configured intrabar interval. Without this, dozens of high-timeframe
/// streams exhaust provider REST budgets (e.g. Binance `-1003` IP bans).
fn effective_confirmed_polling_interval_ms(
    configured_ms: u64,
    timeframe: openticker_core::Timeframe,
) -> u64 {
    let timeframe_ms = u64::try_from(timeframe.duration().as_millis()).unwrap_or(u64::MAX);
    let derived_ms = (timeframe_ms / 4).min(MAX_CONFIRMED_POLL_INTERVAL_MS);
    configured_ms.max(derived_ms)
}

impl Runtime {
    #[must_use]
    pub fn connector_registry(&self) -> Arc<Mutex<ConnectorRegistry>> {
        Arc::clone(&self.connectors)
    }

    #[must_use]
    pub fn effective_streams_for_dataplane(&self) -> Vec<StreamSpec> {
        let mut registry = StreamRegistry::default();

        for (instance_id, instance) in &self.state.lanes {
            registry.insert(StreamSpec {
                key: StreamKey {
                    account_id: instance.config.account.clone(),
                    symbol: instance.lane_symbol.clone(),
                    timeframe: instance.config.timeframe,
                },
                retention: self.catalog.data_plane_config.default_retention,
                polling_interval_ms: effective_confirmed_polling_interval_ms(
                    instance.config.polling_interval_ms,
                    instance.config.timeframe,
                ),
                close_poll_retry_ms: Some(CLOSE_POLL_RETRY_MS),
                close_poll_grace_ms: Some(instance.risk_limits.stale_data_ms),
                preview_enabled: instance_preview_enabled(instance),
                sources: vec![StreamSource::Instance(instance_id.clone())],
            });
        }

        for watch in &self.catalog.data_plane_config.watchlist {
            registry.insert(StreamSpec {
                key: StreamKey {
                    account_id: watch.account.clone(),
                    symbol: watch.symbol.clone(),
                    timeframe: watch.timeframe,
                },
                retention: watch
                    .retention
                    .unwrap_or(self.catalog.data_plane_config.default_retention),
                polling_interval_ms: watch
                    .polling_interval_ms
                    .unwrap_or(self.catalog.data_plane_config.default_polling_interval_ms),
                close_poll_retry_ms: None,
                close_poll_grace_ms: None,
                preview_enabled: false,
                sources: vec![StreamSource::Watchlist],
            });
        }

        registry.specs()
    }

    pub(crate) fn effective_preview_streams_by_account(
        &self,
    ) -> BTreeMap<String, Vec<ConnectorMarketStreamSubscription>> {
        let mut subscriptions = BTreeMap::<String, Vec<ConnectorMarketStreamSubscription>>::new();

        for instance in self.state.lanes.values() {
            if !matches!(instance.state, crate::LaneRuntimeState::Running)
                || !instance_preview_enabled(instance)
            {
                continue;
            }

            subscriptions
                .entry(instance.config.account.clone())
                .or_default()
                .push(ConnectorMarketStreamSubscription {
                    symbol: instance.lane_symbol.clone(),
                    timeframe: instance.config.timeframe,
                });
        }

        for account_subscriptions in subscriptions.values_mut() {
            account_subscriptions.sort_by(|left, right| {
                left.symbol
                    .cmp(&right.symbol)
                    .then_with(|| left.timeframe.to_string().cmp(&right.timeframe.to_string()))
            });
            account_subscriptions.dedup();
        }

        subscriptions
    }

    /// Fetches connector-backed history for a registered dataplane stream.
    ///
    /// # Errors
    ///
    /// Returns an error when the stream is unknown, connector readiness checks
    /// fail, or the connector cannot return history.
    pub fn stream_history(
        &self,
        key: &StreamKey,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ServiceError> {
        let is_registered = self
            .effective_streams_for_dataplane()
            .into_iter()
            .any(|spec| spec.key == *key);
        if !is_registered {
            return Err(ServiceError::InvalidConfiguration(format!(
                "stream `{}/{}/{}` is not registered",
                key.account_id, key.symbol, key.timeframe
            )));
        }

        self.connector_gateway().fetch_recent_bars_for_stream(
            &key.account_id,
            &key.symbol,
            key.timeframe,
            limit.max(1),
        )
    }
}

fn instance_preview_enabled(instance: &crate::LaneRuntime) -> bool {
    matches!(instance.config.signal_mode, SignalMode::Intrabar)
        && matches!(instance.execution_mode, ExecutionMode::Paper)
        && instance.config.data_connector == "binance"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LaneRuntimeState;
    use crate::test_support::fixture_bundle;
    use openticker_core::Timeframe;

    #[test]
    fn instance_stream_polling_cadence_scales_with_timeframe() {
        // Confirmed bars only change once per timeframe; the steady-state
        // REST cadence must not hammer the connector at the intrabar interval.
        let mut config = fixture_bundle();
        config.instances[0].polling_interval_ms = 1_000;
        config.instances[0].timeframe = Timeframe::H4;
        let streams = Runtime::from_config(&config).effective_streams_for_dataplane();
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].polling_interval_ms, 60_000);

        let mut config = fixture_bundle();
        config.instances[0].polling_interval_ms = 1_000;
        config.instances[0].timeframe = Timeframe::M1;
        let streams = Runtime::from_config(&config).effective_streams_for_dataplane();
        assert_eq!(streams[0].polling_interval_ms, 15_000);

        // A slower configured interval is respected as-is.
        let mut config = fixture_bundle();
        config.instances[0].polling_interval_ms = 120_000;
        config.instances[0].timeframe = Timeframe::M1;
        let streams = Runtime::from_config(&config).effective_streams_for_dataplane();
        assert_eq!(streams[0].polling_interval_ms, 120_000);
    }

    #[test]
    fn effective_streams_for_dataplane_merges_instances_and_watchlist() {
        let mut config = fixture_bundle();
        let mut twin = config.instances[0].clone();
        twin.id = "aapl-secondary".to_owned();
        twin.polling_interval_ms = 5_000;
        config.instances.push(twin);
        config
            .global
            .data_plane
            .watchlist
            .push(openticker_config::DataPlaneWatchlistEntry {
                account: "alpaca-paper".to_owned(),
                symbol: "AAPL".to_owned(),
                timeframe: Timeframe::M1,
                polling_interval_ms: Some(2_000),
                retention: Some(1_000),
            });
        config
            .global
            .data_plane
            .watchlist
            .push(openticker_config::DataPlaneWatchlistEntry {
                account: "alpaca-paper".to_owned(),
                symbol: "SPY".to_owned(),
                timeframe: Timeframe::M1,
                polling_interval_ms: None,
                retention: None,
            });

        let runtime = Runtime::from_config(&config);
        let streams = runtime.effective_streams_for_dataplane();

        assert_eq!(streams.len(), 2);
        let aapl = streams
            .iter()
            .find(|spec| spec.key.symbol == "AAPL")
            .expect("AAPL stream should exist");
        assert_eq!(aapl.retention, 1_000);
        // Instances contribute the timeframe-derived cadence (15s for M1);
        // the explicit 2s watchlist entry wins the merge.
        assert_eq!(aapl.polling_interval_ms, 2_000);
        assert_eq!(aapl.close_poll_retry_ms, Some(2_000));
        assert_eq!(aapl.close_poll_grace_ms, Some(3_000));
        assert_eq!(
            aapl.sources,
            vec![
                StreamSource::Watchlist,
                StreamSource::Instance("aapl".to_owned()),
                StreamSource::Instance("aapl-secondary".to_owned()),
            ]
        );

        let spy = streams
            .iter()
            .find(|spec| spec.key.symbol == "SPY")
            .expect("SPY stream should exist");
        assert_eq!(spy.retention, config.global.data_plane.default_retention);
        assert_eq!(
            spy.polling_interval_ms,
            config.global.data_plane.default_polling_interval_ms
        );
        assert_eq!(spy.close_poll_retry_ms, None);
        assert_eq!(spy.close_poll_grace_ms, None);
        assert_eq!(spy.sources, vec![StreamSource::Watchlist]);
    }

    #[test]
    fn multi_symbol_bot_fans_out_into_symbol_lanes() {
        let mut config = fixture_bundle();
        config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];

        let mut runtime = Runtime::from_config(&config);

        let summary = runtime.get_instance("aapl").expect("instance should exist");
        assert_eq!(summary.symbols, vec!["AAPL", "MSFT"]);
        assert_eq!(summary.open_symbol_count, 0);
        assert_eq!(summary.warmup_ready_symbols, 2);

        let lane_summaries = runtime
            .lane_summaries_for_bot("aapl")
            .expect("lane summaries should load");
        assert_eq!(lane_summaries.len(), 2);
        assert_eq!(lane_summaries[0].symbol, "AAPL");
        assert_eq!(lane_summaries[1].symbol, "MSFT");

        let stream_symbols = runtime
            .effective_streams_for_dataplane()
            .into_iter()
            .map(|stream| stream.key.symbol)
            .collect::<Vec<_>>();
        assert_eq!(stream_symbols, vec!["AAPL", "MSFT"]);

        let started = runtime.start_instance("aapl").expect("bot should start");
        assert_eq!(started.state, LaneRuntimeState::Running);
        let lane_summaries = runtime
            .lane_summaries_for_bot("aapl")
            .expect("lane summaries should load");
        assert!(
            lane_summaries
                .iter()
                .all(|lane| matches!(lane.state, LaneRuntimeState::Running))
        );
    }
}
