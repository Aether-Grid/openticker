use crate::repo::RuntimeRepoRead;
use crate::{
    ConnectorAccountSnapshot, ConnectorRuntimeStatus, Runtime, ServiceError,
    connector_runtime_status_from_account_status,
};
use openticker_gateway::{Gateway, GatewayError};

pub(crate) struct ConnectorGatewayRead<'a> {
    gateway: Gateway,
    repo: RuntimeRepoRead<'a>,
}

impl<'a> ConnectorGatewayRead<'a> {
    pub(crate) fn from_parts(gateway: Gateway, repo: RuntimeRepoRead<'a>) -> Self {
        Self { gateway, repo }
    }
}

impl Runtime {
    pub(crate) fn connector_gateway(&self) -> ConnectorGatewayRead<'_> {
        ConnectorGatewayRead {
            gateway: Gateway::new(self.connectors.clone()),
            repo: self.repo(),
        }
    }
}

impl ConnectorGatewayRead<'_> {
    pub(crate) fn gateway(&self) -> &Gateway {
        &self.gateway
    }

    pub(crate) fn repo(&self) -> RuntimeRepoRead<'_> {
        self.repo
    }

    pub(crate) fn account_kind(
        &self,
        lane_id: &str,
        account_id: &str,
    ) -> Result<String, ServiceError> {
        self.gateway
            .account_kind(account_id)
            .map_err(|error| match error {
                GatewayError::UnknownAccount { .. } => ServiceError::InvalidConfiguration(format!(
                    "instance `{lane_id}` references unknown account `{account_id}`"
                )),
                other => ServiceError::InvalidConfiguration(other.to_string()),
            })
    }

    pub(crate) fn validate_connector_kind(
        &self,
        lane_id: &str,
        account_id: &str,
        expected_connector_kind: &str,
        context: &str,
    ) -> Result<(), ServiceError> {
        let account_kind = self.account_kind(lane_id, account_id)?;
        if account_kind != expected_connector_kind {
            return Err(ServiceError::InvalidConfiguration(format!(
                "instance `{lane_id}` {context} `{expected_connector_kind}` does not match account `{account_id}` kind `{account_kind}`"
            )));
        }
        Ok(())
    }

    pub(crate) fn map_status_error(
        lane_id: &str,
        account_id: &str,
        error: GatewayError,
    ) -> ServiceError {
        match error {
            GatewayError::ConnectorNotReady { state, reason, .. } => {
                ServiceError::ConnectorNotReady {
                    instance_id: lane_id.to_owned(),
                    account_id: account_id.to_owned(),
                    state,
                    reason,
                }
            }
            GatewayError::UnknownAccount { .. } => ServiceError::InvalidConfiguration(format!(
                "missing connector status for account `{account_id}`"
            )),
            other => ServiceError::InvalidConfiguration(other.to_string()),
        }
    }

    pub(crate) fn map_data_error(
        instance_id: &str,
        account_id: &str,
        error: GatewayError,
    ) -> ServiceError {
        match error {
            GatewayError::LockPoisoned => ServiceError::InvalidConfiguration(error.to_string()),
            GatewayError::ConnectorNotReady { state, reason, .. } => {
                ServiceError::ConnectorNotReady {
                    instance_id: instance_id.to_owned(),
                    account_id: account_id.to_owned(),
                    state,
                    reason,
                }
            }
            other => ServiceError::DataConnectorUnavailable {
                instance_id: instance_id.to_owned(),
                account_id: account_id.to_owned(),
                reason: other.to_string(),
            },
        }
    }

    pub(crate) fn map_execution_error(
        instance_id: &str,
        account_id: &str,
        error: GatewayError,
    ) -> ServiceError {
        match error {
            GatewayError::LockPoisoned => ServiceError::InvalidConfiguration(error.to_string()),
            other => ServiceError::ExecutionConnectorUnavailable {
                instance_id: instance_id.to_owned(),
                account_id: account_id.to_owned(),
                reason: other.to_string(),
            },
        }
    }

    pub(crate) fn runtime_statuses(&self) -> Result<Vec<ConnectorRuntimeStatus>, ServiceError> {
        Ok(self
            .gateway
            .statuses()
            .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?
            .into_iter()
            .map(connector_runtime_status_from_account_status)
            .collect())
    }

    pub(crate) fn connectors_ready(&self) -> Result<bool, ServiceError> {
        self.gateway
            .is_ready()
            .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))
    }

    pub(crate) fn ensure_account_connector_ready(
        &self,
        lane_id: &str,
        account_id: &str,
    ) -> Result<(), ServiceError> {
        self.gateway
            .ensure_account_ready(account_id)
            .map_err(|error| Self::map_status_error(lane_id, account_id, error))
    }

    pub(crate) fn fetch_account_snapshot_for_refresh(
        &self,
        account_id: &str,
    ) -> Result<ConnectorAccountSnapshot, ServiceError> {
        self.gateway
            .fetch_account_snapshot_unchecked(account_id)
            .map_err(|error| match error {
                GatewayError::UnknownAccount { .. } => ServiceError::InvalidConfiguration(format!(
                    "missing connector status for account `{account_id}`"
                )),
                GatewayError::LockPoisoned => ServiceError::InvalidConfiguration(
                    "connector registry lock poisoned".to_owned(),
                ),
                other => ServiceError::ExecutionConnectorUnavailable {
                    instance_id: account_id.to_owned(),
                    account_id: account_id.to_owned(),
                    reason: other.to_string(),
                },
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fixture_bundle;
    use crate::{ConnectionState, LaneRuntimeState, MarketType};

    #[test]
    fn exposes_connector_runtime_statuses() {
        let config = fixture_bundle();
        let runtime = Runtime::from_config(&config);

        let statuses = runtime.connector_statuses();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].account_id, "alpaca-paper");
        assert_eq!(statuses[0].kind, "alpaca");
        assert!(matches!(statuses[0].state, ConnectionState::Connected));
        assert_eq!(statuses[0].resilience_policy.reconnect_base_backoff_ms, 500);
        assert_eq!(statuses[0].resilience_state.consecutive_failures, 0);
    }

    #[test]
    fn blocks_start_when_connector_is_not_ready() {
        let mut config = fixture_bundle();
        config.accounts[0].kind = "binance".to_owned();
        config.accounts[0].use_demo_mode = false;

        let mut runtime = Runtime::from_config(&config);
        assert!(!runtime.is_ready());

        let result = runtime.start_instance("aapl");
        assert!(matches!(
            result,
            Err(ServiceError::ConnectorNotReady { reason, .. })
                if reason.contains("consecutive_failures=1")
                    && reason.contains("next_reconnect_at_ms=")
        ));
    }

    #[test]
    fn blocks_start_when_binance_paper_account_omits_demo_mode() {
        let mut config = fixture_bundle();
        config.accounts[0].kind = "binance".to_owned();
        config.accounts[0].use_demo_mode = false;
        config.instances[0].market = MarketType::Crypto;
        config.instances[0].symbols = vec!["BTCUSDT".to_owned()];
        config.instances[0].data_connector = "binance".to_owned();
        config.instances[0].execution_connector = "binance".to_owned();

        let mut runtime = Runtime::from_config(&config);
        assert!(!runtime.is_ready());

        let result = runtime.start_instance("aapl");
        assert!(matches!(
            result,
            Err(ServiceError::ConnectorNotReady { .. })
        ));
    }

    #[test]
    fn allows_start_when_binance_paper_account_uses_demo_mode() {
        let mut config = fixture_bundle();
        config.accounts[0].kind = "binance".to_owned();
        config.accounts[0].use_demo_mode = true;
        config.instances[0].market = MarketType::Crypto;
        config.instances[0].symbols = vec!["BTCUSDT".to_owned()];
        config.instances[0].data_connector = "binance".to_owned();
        config.instances[0].execution_connector = "binance".to_owned();

        let mut runtime = Runtime::from_config(&config);
        assert!(runtime.is_ready());

        let started = runtime.start_instance("aapl").unwrap();
        assert_eq!(started.state, LaneRuntimeState::Running);
    }
}
