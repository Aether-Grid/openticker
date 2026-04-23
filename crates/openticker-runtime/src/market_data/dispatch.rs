use super::gateway::{
    GatewayFetchFailure, PendingProviderEvent, append_pending_provider_events,
    gateway_fetch_latest_bar_with_events,
};
use super::recovery::LanePollingAdvance;
use crate::{
    LaneRuntimeState, OhlcvBar, ProcessBarOutcome, Runtime, ServiceError, SignalPhase, StreamKey,
    instance_matches_stream_key,
};
use openticker_gateway::Gateway;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) enum StreamPollPlan {
    BareFetch {
        stream_id: String,
        account_id: String,
        account_kind: String,
        symbol: String,
        timeframe: crate::Timeframe,
    },
    LaneFanOut {
        instance_ids: Vec<String>,
    },
}

#[derive(Debug)]
pub(crate) struct BareStreamFetchExecution {
    pub(crate) bar: OhlcvBar,
    pub(crate) provider_events: Vec<PendingProviderEvent>,
}

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

    #[allow(dead_code)]
    pub(crate) fn advance_stream_polling_once(
        &mut self,
        key: &StreamKey,
    ) -> Result<LanePollingAdvance, ServiceError> {
        let plan = self.plan_stream_polling(key)?;
        match plan {
            StreamPollPlan::BareFetch {
                stream_id,
                account_id,
                account_kind,
                symbol,
                timeframe,
            } => {
                let gateway = self.connector_gateway_snapshot();
                let execution = match Runtime::execute_bare_stream_fetch(
                    &gateway,
                    &stream_id,
                    &account_id,
                    &account_kind,
                    &symbol,
                    timeframe,
                ) {
                    Ok(execution) => execution,
                    Err(failure) => {
                        append_pending_provider_events(self, &failure.provider_events)?;
                        return Err(failure.error);
                    }
                };
                self.apply_bare_fetched_bar(key, execution.bar, &execution.provider_events)
            }
            StreamPollPlan::LaneFanOut { instance_ids } => self.apply_lane_fanout(&instance_ids),
        }
    }

    pub(crate) fn plan_stream_polling(
        &self,
        key: &StreamKey,
    ) -> Result<StreamPollPlan, ServiceError> {
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
            return Ok(StreamPollPlan::BareFetch {
                stream_id: format!("stream:{}/{}/{}", key.account_id, key.symbol, key.timeframe),
                account_id: key.account_id.clone(),
                account_kind: account.kind.clone(),
                symbol: key.symbol.clone(),
                timeframe: key.timeframe,
            });
        }

        Ok(StreamPollPlan::LaneFanOut {
            instance_ids: matching_instance_ids,
        })
    }

    pub(crate) fn execute_bare_stream_fetch(
        gateway: &Gateway,
        stream_id: &str,
        account_id: &str,
        account_kind: &str,
        symbol: &str,
        timeframe: crate::Timeframe,
    ) -> Result<BareStreamFetchExecution, GatewayFetchFailure> {
        let (bar, provider_events) = gateway_fetch_latest_bar_with_events(
            gateway,
            stream_id,
            account_id,
            account_kind,
            symbol,
            timeframe,
        )?;
        Ok(BareStreamFetchExecution {
            bar,
            provider_events,
        })
    }

    pub(crate) fn apply_bare_fetched_bar(
        &mut self,
        _key: &StreamKey,
        bar: OhlcvBar,
        provider_events: &[PendingProviderEvent],
    ) -> Result<LanePollingAdvance, ServiceError> {
        append_pending_provider_events(self, provider_events)?;
        Ok(LanePollingAdvance {
            recorded_bars: vec![bar],
            outcomes: Vec::new(),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn apply_lane_fanout(
        &mut self,
        instance_ids: &[String],
    ) -> Result<LanePollingAdvance, ServiceError> {
        let mut bars_by_timestamp = BTreeMap::new();
        let mut outcomes = Vec::new();
        for instance_id in instance_ids {
            let advance = self.advance_lane_polling_once(
                instance_id.as_str(),
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
