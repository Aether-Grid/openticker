use super::recovery::LanePollingAdvance;
use crate::{
    LaneRuntimeState, OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, SignalPhase, StreamKey,
    instance_matches_stream_key,
};
use std::collections::BTreeMap;

impl Runtime {
    /// Dispatches a fetched bar to matching running instances.
    ///
    /// # Errors
    ///
    /// Returns an error when journal appends or bar processing fail unexpectedly.
    pub fn dispatch_bar(
        &mut self,
        key: &StreamKey,
        bar: &OhlcvBar,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        if self.state.kill_switch_active {
            return Ok(Vec::new());
        }

        let matching_instance_ids = self
            .state
            .lanes
            .iter()
            .filter_map(|(instance_id, instance)| {
                if !matches!(instance.state, LaneRuntimeState::Running) {
                    return None;
                }
                if instance_matches_stream_key(instance, key) {
                    Some(instance_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut outcomes = Vec::new();
        for instance_id in matching_instance_ids {
            if let Some(outcome) =
                self.process_fetched_confirmed_bar(&instance_id, &key.symbol, bar)?
            {
                outcomes.push(outcome);
            }
        }

        Ok(outcomes)
    }

    pub(crate) fn process_fetched_confirmed_bar(
        &mut self,
        instance_id: &str,
        symbol: &str,
        latest_bar: &OhlcvBar,
    ) -> Result<Option<ProcessBarOutcome>, ServiceError> {
        if self
            .instance(instance_id)?
            .last_dispatched_bar_timestamp
            .is_some_and(|previous| previous >= latest_bar.timestamp)
        {
            return Ok(None);
        }

        self.append_runtime_event(
            "poll",
            Some(instance_id),
            "poll.bar_received",
            format!(
                "symbol={symbol},bar_timestamp={bar_timestamp},close={close}",
                bar_timestamp = latest_bar.timestamp.to_rfc3339(),
                close = latest_bar.close,
            ),
        )?;

        self.process_bar_for_lane(instance_id, latest_bar, SignalPhase::Confirmed)
            .map(Some)
    }

    pub(crate) fn advance_stream_polling_once(
        &mut self,
        key: &StreamKey,
    ) -> Result<LanePollingAdvance, ServiceError> {
        let matching_instance_ids = self
            .state
            .lanes
            .iter()
            .filter_map(|(instance_id, instance)| {
                if !matches!(instance.state, LaneRuntimeState::Running) {
                    return None;
                }
                if instance_matches_stream_key(instance, key) {
                    Some(instance_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if self.state.kill_switch_active || matching_instance_ids.is_empty() {
            let account = self.catalog.accounts.get(&key.account_id).ok_or_else(|| {
                ServiceError::InvalidConfiguration(format!(
                    "stream `{}/{}/{}` references unknown account `{}`",
                    key.account_id, key.symbol, key.timeframe, key.account_id
                ))
            })?;
            let stream_id = format!("stream:{}/{}/{}", key.account_id, key.symbol, key.timeframe);
            let latest_bar = self.connector_gateway().fetch_latest_bar(
                &stream_id,
                &key.account_id,
                account.kind.as_str(),
                &key.symbol,
                key.timeframe,
            )?;
            return Ok(LanePollingAdvance {
                recorded_bars: vec![latest_bar],
                outcomes: Vec::new(),
            });
        }

        let mut bars_by_timestamp = BTreeMap::new();
        let mut outcomes = Vec::new();
        for instance_id in matching_instance_ids {
            let advance = self.advance_lane_polling_once(
                &instance_id,
                super::recovery::RECOVERY_PAGE_LIMIT,
                super::recovery::MAX_RECOVERY_PAGES_PER_CYCLE,
            )?;
            for bar in advance.recorded_bars {
                bars_by_timestamp.insert(bar.timestamp, bar);
            }
            outcomes.extend(advance.outcomes);
        }

        Ok(LanePollingAdvance {
            recorded_bars: bars_by_timestamp.into_values().collect(),
            outcomes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_bundle, test_bar_at};
    use openticker_core::Timeframe;

    #[test]
    fn dispatch_bar_only_routes_to_running_instances_and_respects_kill_switch() {
        let mut config = fixture_bundle();
        let mut twin = config.instances[0].clone();
        twin.id = "aapl-secondary".to_owned();
        config.instances.push(twin);

        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let key = StreamKey {
            account_id: "alpaca-paper".to_owned(),
            symbol: "AAPL".to_owned(),
            timeframe: Timeframe::M1,
        };

        let outcomes = runtime
            .dispatch_bar(&key, &test_bar_at("2030-01-01T00:00:00Z", 100.0))
            .expect("dispatch should succeed");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].instance_id, "aapl");

        runtime
            .set_kill_switch(true)
            .expect("kill switch should toggle");
        let halted = runtime
            .dispatch_bar(&key, &test_bar_at("2030-01-01T00:01:00Z", 101.0))
            .expect("dispatch should still succeed");
        assert!(halted.is_empty());
    }
}
