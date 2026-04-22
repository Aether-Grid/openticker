use crate::{
    LaneRuntimeState, NormalizedBarUpdate, NormalizedTrade, ProcessBarOutcome, Runtime,
    ServiceError, SignalPhase, StreamKey, instance_matches_stream_key,
};

impl Runtime {
    pub(crate) fn process_market_stream_update_for_stream(
        &mut self,
        key: &StreamKey,
        stream_update: &NormalizedBarUpdate,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        if self.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        let matching_lane_ids = self
            .state
            .lanes
            .iter()
            .filter_map(|(lane_id, instance)| {
                if !matches!(instance.state, LaneRuntimeState::Running) {
                    return None;
                }
                if instance_matches_stream_key(instance, key) {
                    Some(lane_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut outcomes = Vec::new();
        for lane_id in matching_lane_ids {
            if matches!(stream_update.phase, SignalPhase::Confirmed)
                && self
                    .instance(&lane_id)?
                    .last_dispatched_bar_timestamp
                    .is_some_and(|previous| previous >= stream_update.bar.timestamp)
            {
                continue;
            }

            {
                let instance = self.instance_mut(&lane_id)?;
                if instance
                    .last_stream_update
                    .as_ref()
                    .is_some_and(|previous| previous == stream_update)
                {
                    continue;
                }
                instance.last_stream_update = Some(stream_update.clone());
            }

            outcomes.push(self.process_bar_for_lane(
                &lane_id,
                &stream_update.bar,
                stream_update.phase,
            )?);
        }

        Ok(outcomes)
    }

    /// Ingests a normalized trade into an instance and emits any completed bar outcomes.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance cannot accept the trade or downstream processing fails.
    pub fn process_trade(
        &mut self,
        instance_id: &str,
        trade: &NormalizedTrade,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        let lane_id = match self.resolve_lane_id(instance_id, &trade.symbol) {
            Ok(lane_id) => lane_id,
            Err(ServiceError::SymbolNotConfigured { .. }) => {
                let expected = self.get_instance(instance_id)?.symbols.join(",");
                return Err(ServiceError::TradeSymbolMismatch {
                    instance_id: instance_id.to_owned(),
                    expected,
                    received: trade.symbol.clone(),
                });
            }
            Err(error) => return Err(error),
        };
        let account_id = self.instance(&lane_id)?.config.account.clone();
        self.connector_gateway()
            .ensure_account_connector_ready(&lane_id, &account_id)?;

        let updates = {
            let instance = self.instance_mut(&lane_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }
            if !matches!(instance.state, LaneRuntimeState::Running) {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: instance.state,
                    action: "process_trade",
                });
            }

            let expected_symbol = instance.lane_symbol.as_str();
            if trade.symbol != expected_symbol {
                return Err(ServiceError::TradeSymbolMismatch {
                    instance_id: instance_id.to_owned(),
                    expected: expected_symbol.to_owned(),
                    received: trade.symbol.clone(),
                });
            }

            instance.bar_builder.push_trade(trade)?
        };

        let mut outcomes = Vec::new();
        for update in updates {
            outcomes.push(self.process_bar_for_lane(&lane_id, &update.bar, update.phase)?);
        }
        Ok(outcomes)
    }

    /// Processes a raw market stream payload and returns resulting bar outcomes.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload cannot be normalized or instance constraints are violated.
    #[allow(clippy::too_many_lines)]
    pub fn process_market_stream_payload(
        &mut self,
        instance_id: &str,
        payload: &str,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        if self.state.kill_switch_active {
            return Err(ServiceError::KillSwitchEnabled);
        }

        let lane_id = self.single_lane_id_for_bot(instance_id, "process_market_stream_payload")?;

        let (account_id, data_connector, expected_symbol) = {
            let instance = self.instance(&lane_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }
            if !matches!(instance.state, LaneRuntimeState::Running) {
                return Err(ServiceError::InvalidTransition {
                    instance_id: instance_id.to_owned(),
                    state: instance.state,
                    action: "process_market_stream_payload",
                });
            }

            let expected_symbol = instance.lane_symbol.clone();

            (
                instance.config.account.clone(),
                instance.config.data_connector.clone(),
                expected_symbol,
            )
        };

        self.connector_gateway()
            .ensure_account_connector_ready(&lane_id, &account_id)?;
        let stream_update = self.connector_gateway().normalize_market_stream_payload(
            &lane_id,
            &account_id,
            &data_connector,
            payload,
        )?;
        let Some(stream_update) = stream_update else {
            return Ok(Vec::new());
        };

        if stream_update.symbol != expected_symbol {
            return Err(ServiceError::TradeSymbolMismatch {
                instance_id: instance_id.to_owned(),
                expected: expected_symbol,
                received: stream_update.symbol,
            });
        }

        self.process_market_stream_update_for_stream(
            &StreamKey {
                account_id,
                symbol: expected_symbol,
                timeframe: self.instance(&lane_id)?.config.timeframe,
            },
            &stream_update,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_bundle, test_bar_at, test_trade};

    #[test]
    fn process_trade_routes_through_bar_builder() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let first = runtime
            .process_trade(
                "aapl",
                &test_trade("AAPL", "2030-01-01T00:00:05Z", 100.0, 1.0),
            )
            .expect("first trade should process");
        assert_eq!(first.len(), 1);

        let second = runtime
            .process_trade(
                "aapl",
                &test_trade("AAPL", "2030-01-01T00:01:00Z", 101.0, 1.0),
            )
            .expect("second trade should process");
        assert_eq!(second.len(), 2);
    }

    #[test]
    fn process_trade_rejects_symbol_mismatch() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let result = runtime.process_trade(
            "aapl",
            &test_trade("MSFT", "2030-01-01T00:00:05Z", 100.0, 1.0),
        );
        assert!(matches!(
            result,
            Err(ServiceError::TradeSymbolMismatch { .. })
        ));
    }

    #[test]
    fn market_stream_updates_fan_out_to_the_matching_symbol_lane() {
        let mut config = fixture_bundle();
        config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];
        let mut runtime = Runtime::from_config(&config);
        runtime.start_instance("aapl").expect("bot should start");

        let outcomes = runtime
            .process_market_stream_update_for_stream(
                &StreamKey {
                    account_id: "alpaca-paper".to_owned(),
                    symbol: "MSFT".to_owned(),
                    timeframe: crate::Timeframe::M1,
                },
                &NormalizedBarUpdate {
                    symbol: "MSFT".to_owned(),
                    phase: SignalPhase::Confirmed,
                    bar: test_bar_at("2030-01-01T00:01:00Z", 101.0),
                },
            )
            .expect("stream update should process");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].bot_id, "aapl");
        assert_eq!(outcomes[0].symbol, "MSFT");
    }

    #[test]
    fn market_stream_updates_fan_out_to_all_matching_bots_on_a_shared_stream() {
        let mut config = fixture_bundle();
        let mut twin = config.instances[0].clone();
        twin.id = "aapl-secondary".to_owned();
        config.instances.push(twin);
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("primary bot should start");
        runtime
            .start_instance("aapl-secondary")
            .expect("secondary bot should start");

        let outcomes = runtime
            .process_market_stream_update_for_stream(
                &StreamKey {
                    account_id: "alpaca-paper".to_owned(),
                    symbol: "AAPL".to_owned(),
                    timeframe: crate::Timeframe::M1,
                },
                &NormalizedBarUpdate {
                    symbol: "AAPL".to_owned(),
                    phase: SignalPhase::Confirmed,
                    bar: test_bar_at("2030-01-01T00:02:00Z", 102.0),
                },
            )
            .expect("shared stream update should process");

        assert_eq!(outcomes.len(), 2);
        let mut bot_ids = outcomes
            .into_iter()
            .map(|outcome| outcome.bot_id)
            .collect::<Vec<_>>();
        bot_ids.sort();
        assert_eq!(
            bot_ids,
            vec!["aapl".to_owned(), "aapl-secondary".to_owned()]
        );
    }
}
