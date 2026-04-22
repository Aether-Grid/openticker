use crate::{LaneRuntimeState, OhlcvBar, Runtime, ServiceError, StreamKey, Timeframe};

impl Runtime {
    pub(super) fn manual_poll_target_for_instance(
        &self,
        instance_id: &str,
        action: &'static str,
    ) -> Result<(String, String, String, Timeframe), ServiceError> {
        let instance = self.instance(instance_id)?;
        if !instance.config.enabled {
            return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
        }
        if !matches!(instance.state, LaneRuntimeState::Running) {
            return Err(ServiceError::InvalidTransition {
                instance_id: instance_id.to_owned(),
                state: instance.state,
                action,
            });
        }

        let symbol = instance.lane_symbol.clone();

        Ok((
            instance.config.account.clone(),
            instance.config.data_connector.clone(),
            symbol,
            instance.config.timeframe,
        ))
    }

    /// Fetches the latest bar for the provided instance from its data connector.
    ///
    /// # Errors
    ///
    /// Returns an error when instance validation fails, connector readiness checks fail,
    /// or connector polling fails.
    pub fn fetch_latest_bar_for_instance(
        &mut self,
        instance_id: &str,
    ) -> Result<(StreamKey, OhlcvBar), ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "poll_instance_once")?;
        self.fetch_latest_bar_for_lane(&lane_id)
    }

    /// Fetches the latest bar for a specific symbol lane from its data connector.
    ///
    /// # Errors
    ///
    /// Returns an error when lane resolution fails, connector readiness checks
    /// fail, or connector polling fails.
    pub fn fetch_latest_bar_for_symbol(
        &mut self,
        instance_id: &str,
        symbol: &str,
    ) -> Result<(StreamKey, OhlcvBar), ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.fetch_latest_bar_for_lane(&lane_id)
    }

    pub(crate) fn fetch_latest_bar_for_lane(
        &mut self,
        instance_id: &str,
    ) -> Result<(StreamKey, OhlcvBar), ServiceError> {
        let (account_id, data_connector, symbol, timeframe) =
            self.manual_poll_target_for_instance(instance_id, "poll_instance_once")?;
        self.connector_gateway()
            .ensure_account_connector_ready(instance_id, &account_id)?;
        let latest_bar = self.connector_gateway().fetch_latest_bar(
            instance_id,
            &account_id,
            &data_connector,
            &symbol,
            timeframe,
        )?;

        Ok((
            StreamKey {
                account_id,
                symbol,
                timeframe,
            },
            latest_bar,
        ))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn fetch_recent_bars_for_instance(
        &mut self,
        instance_id: &str,
        limit: usize,
    ) -> Result<(StreamKey, Vec<OhlcvBar>), ServiceError> {
        let (account_id, data_connector, symbol, timeframe) = {
            let instance = self.instance(instance_id)?;
            if !instance.config.enabled {
                return Err(ServiceError::InstanceDisabled(instance_id.to_owned()));
            }

            let symbol = instance.lane_symbol.clone();

            (
                instance.config.account.clone(),
                instance.config.data_connector.clone(),
                symbol,
                instance.config.timeframe,
            )
        };

        let bars = self.connector_gateway().fetch_recent_bars(
            instance_id,
            &account_id,
            &data_connector,
            &symbol,
            timeframe,
            limit,
        )?;

        Ok((
            StreamKey {
                account_id,
                symbol,
                timeframe,
            },
            bars,
        ))
    }

    /// Returns the dataplane key used to poll the provided instance.
    ///
    /// # Errors
    ///
    /// Returns an error when the instance is unknown, disabled, or not in a pollable state.
    pub fn stream_key_for_instance(&self, instance_id: &str) -> Result<StreamKey, ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "poll_instance_once")?;
        self.stream_key_for_lane(&lane_id)
    }

    /// Returns the dataplane key used to poll a specific symbol lane.
    ///
    /// # Errors
    ///
    /// Returns an error when the symbol lane cannot be resolved or is not in a
    /// pollable state.
    pub fn stream_key_for_symbol(
        &self,
        instance_id: &str,
        symbol: &str,
    ) -> Result<StreamKey, ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.stream_key_for_lane(&lane_id)
    }

    fn stream_key_for_lane(&self, instance_id: &str) -> Result<StreamKey, ServiceError> {
        let (account_id, _, symbol, timeframe) =
            self.manual_poll_target_for_instance(instance_id, "poll_instance_once")?;
        Ok(StreamKey {
            account_id,
            symbol,
            timeframe,
        })
    }
}
