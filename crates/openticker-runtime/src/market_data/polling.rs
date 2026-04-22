use super::recovery::{LanePollingAdvance, MAX_RECOVERY_PAGES_PER_CYCLE, RECOVERY_PAGE_LIMIT};
use crate::{PollAdvanceResult, ProcessBarOutcome, Runtime, ServiceError, unix_now_ms};

impl Runtime {
    fn record_poll_result_events(
        &mut self,
        instance_id: &str,
        outcomes: &[ProcessBarOutcome],
        error: Option<&ServiceError>,
    ) {
        if !outcomes.is_empty() {
            let _ = self.append_runtime_event(
                "poll",
                Some(instance_id),
                "poll.processed",
                format!("outcomes={}", outcomes.len()),
            );
        }

        if let Some(error) = error {
            let _ = self.append_runtime_event(
                "poll",
                Some(instance_id),
                "poll.failed",
                format!("error={error}"),
            );
        }
    }

    fn poll_instance_once_internal(
        &mut self,
        instance_id: &str,
    ) -> Result<LanePollingAdvance, ServiceError> {
        self.advance_lane_polling_once(
            instance_id,
            RECOVERY_PAGE_LIMIT,
            MAX_RECOVERY_PAGES_PER_CYCLE,
        )
    }

    /// Polls the connector once for the latest bar and processes it.
    ///
    /// # Errors
    ///
    /// Returns an error when polling fails or the instance cannot process the fetched bar.
    pub fn poll_instance_once(
        &mut self,
        instance_id: &str,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "poll_instance_once")?;
        self.poll_instance_once_at(&lane_id, unix_now_ms())
    }

    /// Polls the connector once and returns both tradable outcomes and the bars
    /// that should be recorded into the dataplane buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when polling fails or the instance cannot process the fetched bars.
    pub fn poll_instance_once_detailed(
        &mut self,
        instance_id: &str,
    ) -> Result<PollAdvanceResult, ServiceError> {
        let lane_id = self.single_lane_id_for_bot(instance_id, "poll_instance_once")?;
        self.poll_instance_once_detailed_at(&lane_id, unix_now_ms())
    }

    /// Polls the connector once for the latest bar for a specific symbol lane.
    ///
    /// # Errors
    ///
    /// Returns an error when lane resolution fails, polling fails, or the
    /// instance cannot process the fetched bar.
    pub fn poll_instance_symbol_once(
        &mut self,
        instance_id: &str,
        symbol: &str,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.poll_instance_once_at(&lane_id, unix_now_ms())
    }

    /// Polls a specific symbol lane once and returns both tradable outcomes and
    /// dataplane-recordable bars.
    ///
    /// # Errors
    ///
    /// Returns an error when lane resolution fails, polling fails, or the
    /// instance cannot process the fetched bars.
    pub fn poll_instance_symbol_once_detailed(
        &mut self,
        instance_id: &str,
        symbol: &str,
    ) -> Result<PollAdvanceResult, ServiceError> {
        let lane_id = self.resolve_lane_id(instance_id, symbol)?;
        self.poll_instance_once_detailed_at(&lane_id, unix_now_ms())
    }

    fn poll_instance_once_at(
        &mut self,
        instance_id: &str,
        _now_ms: i64,
    ) -> Result<Vec<ProcessBarOutcome>, ServiceError> {
        let result = self.poll_instance_once_with_recorded_bars_at(instance_id, unix_now_ms());
        match &result {
            Ok(advance) => self.record_poll_result_events(instance_id, &advance.outcomes, None),
            Err(error) => self.record_poll_result_events(instance_id, &[], Some(error)),
        }

        result.map(|advance| advance.outcomes)
    }

    pub(crate) fn poll_instance_once_with_recorded_bars_at(
        &mut self,
        instance_id: &str,
        _now_ms: i64,
    ) -> Result<LanePollingAdvance, ServiceError> {
        self.poll_instance_once_internal(instance_id)
    }

    fn poll_instance_once_detailed_at(
        &mut self,
        instance_id: &str,
        _now_ms: i64,
    ) -> Result<PollAdvanceResult, ServiceError> {
        let advance = self.poll_instance_once_internal(instance_id)?;
        Ok(PollAdvanceResult {
            outcomes: advance.outcomes,
            recorded_bars: advance.recorded_bars,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_bundle;
    use openticker_core::Timeframe;

    #[test]
    fn poll_instance_once_dedupes_latest_confirmed_bar_after_warmup() {
        let mut config = fixture_bundle();
        config.instances[0].timeframe = Timeframe::D1;
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let first = runtime
            .poll_instance_once("aapl")
            .expect("first poll should succeed");
        assert!(first.is_empty());

        let second = runtime
            .poll_instance_once("aapl")
            .expect("second poll should succeed");
        assert!(second.is_empty());

        let provider_events = runtime
            .recent_events_by_scope("provider", 20)
            .expect("provider events should load");
        assert!(provider_events.iter().any(|event| {
            event.kind == "provider.market_data.confirmed_target.requested"
                && event.entity_id.as_deref() == Some("aapl")
        }));
        assert!(provider_events.iter().any(|event| {
            event.kind == "provider.market_data.confirmed_target.succeeded"
                && event.entity_id.as_deref() == Some("aapl")
        }));
    }

    #[test]
    fn poll_instance_once_rejects_data_connector_account_mismatch() {
        let mut config = fixture_bundle();
        config.instances[0].data_connector = "binance".to_owned();
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let result = runtime.poll_instance_once("aapl");
        assert!(matches!(
            result,
            Err(ServiceError::InvalidConfiguration(message)) if message.contains("data_connector")
        ));
    }

    #[test]
    fn poll_instance_once_skips_processed_events_for_duplicate_confirmed_bar() {
        let config = fixture_bundle();
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let first = runtime
            .poll_instance_once("aapl")
            .expect("first poll should succeed");
        assert!(first.is_empty());

        let second = runtime
            .poll_instance_once("aapl")
            .expect("second poll should succeed");
        assert!(second.is_empty());

        let poll_events = runtime
            .recent_events_by_scope("poll", 20)
            .expect("poll events should load");
        assert_eq!(
            poll_events
                .iter()
                .filter(|event| event.kind == "poll.processed")
                .count(),
            0
        );
        assert_eq!(
            poll_events
                .iter()
                .filter(|event| event.kind == "poll.bar_received")
                .count(),
            0
        );
    }

    #[test]
    fn poll_instance_once_records_failed_fetches() {
        let mut config = fixture_bundle();
        config.instances[0].data_connector = "binance".to_owned();
        let mut runtime = Runtime::from_config(&config);
        runtime
            .start_instance("aapl")
            .expect("instance should start");

        let result = runtime.poll_instance_once("aapl");
        assert!(matches!(
            result,
            Err(ServiceError::InvalidConfiguration(message)) if message.contains("data_connector")
        ));

        let poll_events = runtime
            .recent_events_by_scope("poll", 20)
            .expect("poll events should load");
        assert!(
            poll_events.iter().any(
                |event| event.kind == "poll.failed" && event.payload.contains("data_connector")
            )
        );
    }
}
