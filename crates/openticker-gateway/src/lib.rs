use chrono::{DateTime, Utc};
use openticker_config::{AccountConfig, ExecutionConstraintsConfig};
use openticker_connectors::{
    ConfirmedBarPage, ConnectionState, ConnectorAccountSnapshot, ConnectorAccountStatus,
    ConnectorClientHandle, ConnectorError, ConnectorKind, ConnectorPreviewStreamSession,
    ConnectorRegistry, ConnectorSymbolConstraints,
};
use openticker_core::{OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{AcceptedOrder, ExecutionRequest};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Fallback throttle window applied when a provider reports a rate limit
/// without a usable retry-after hint.
const DEFAULT_THROTTLE_WINDOW_MS: u64 = 60_000;

#[derive(Debug, Clone)]
pub struct Gateway {
    connectors: Arc<Mutex<ConnectorRegistry>>,
}

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("connector registry lock poisoned")]
    LockPoisoned,
    #[error("account `{account_id}` has unsupported connector kind `{kind}`")]
    UnsupportedConnectorKind { account_id: String, kind: String },
    #[error("connector account `{account_id}` is not registered")]
    UnknownAccount { account_id: String },
    #[error("connector account `{account_id}` is not ready (`{state:?}`): {reason}")]
    ConnectorNotReady {
        account_id: String,
        state: ConnectionState,
        reason: String,
    },
    #[error(transparent)]
    Connector(ConnectorError),
}

#[derive(Debug, Clone, Default)]
pub struct NormalizedSymbolConstraints {
    pub execution_constraints: ExecutionConstraintsConfig,
    pub fractional_entry_supported: Option<bool>,
    pub source: Option<String>,
}

impl NormalizedSymbolConstraints {
    #[must_use]
    pub fn has_numeric_constraints(&self) -> bool {
        self.execution_constraints.quantity_step.is_some()
            || self.execution_constraints.min_quantity.is_some()
            || self.execution_constraints.min_notional_usd.is_some()
    }
}

impl From<ConnectorError> for GatewayError {
    fn from(error: ConnectorError) -> Self {
        match error {
            ConnectorError::UnknownAccount { account_id } => Self::UnknownAccount { account_id },
            other => Self::Connector(other),
        }
    }
}

impl Gateway {
    #[must_use]
    pub fn new(connectors: Arc<Mutex<ConnectorRegistry>>) -> Self {
        Self { connectors }
    }

    fn lock_connectors(&self) -> Result<MutexGuard<'_, ConnectorRegistry>, GatewayError> {
        self.connectors
            .lock()
            .map_err(|_| GatewayError::LockPoisoned)
    }

    /// Resolves a detached client handle for a connected account, holding the
    /// registry lock only for the lookup so network calls run outside it.
    ///
    /// By design there is no retry window between the caller's
    /// `ensure_account_ready` check and the handle resolved here: if the
    /// account is dropped or disconnected in that gap, the resolution fails
    /// and the error propagates without an automatic retry. Callers that need
    /// resilience must re-invoke the readiness path.
    fn connected_client(&self, account_id: &str) -> Result<ConnectorClientHandle, GatewayError> {
        let connectors = self.lock_connectors()?;
        connectors.connected_client(account_id).map_err(Into::into)
    }

    /// Resolves a detached client handle without a connection-state pre-check.
    fn client_unchecked(&self, account_id: &str) -> Result<ConnectorClientHandle, GatewayError> {
        let connectors = self.lock_connectors()?;
        connectors.client_unchecked(account_id).map_err(Into::into)
    }

    /// Throttles the account when a connector operation reports a rate limit,
    /// so subsequent readiness checks fail fast until the window passes.
    fn track_rate_limit<T>(
        &self,
        account_id: &str,
        result: Result<T, ConnectorError>,
    ) -> Result<T, GatewayError> {
        if let Err(ConnectorError::RateLimited { retry_after_ms, .. }) = &result {
            let throttle_window_ms = retry_after_ms.unwrap_or(DEFAULT_THROTTLE_WINDOW_MS);
            if let Ok(connectors) = self.lock_connectors()
                && let Err(error) = connectors.note_account_rate_limit(
                    account_id,
                    unix_now_ms(),
                    throttle_window_ms,
                )
            {
                tracing::warn!(
                    account_id,
                    %error,
                    "failed to record connector rate-limit; throttle state may be stale",
                );
            }
        }
        result.map_err(Into::into)
    }

    /// Returns connector statuses for all configured accounts.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired.
    pub fn statuses(&self) -> Result<Vec<ConnectorAccountStatus>, GatewayError> {
        let connectors = self.lock_connectors()?;
        Ok(connectors.statuses())
    }

    /// Returns whether every configured connector account is currently ready.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired.
    pub fn is_ready(&self) -> Result<bool, GatewayError> {
        let connectors = self.lock_connectors()?;
        Ok(connectors.is_ready())
    }

    /// Returns the connector kind configured for an account.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired, or [`GatewayError::UnknownAccount`] when the account
    /// is not registered.
    pub fn account_kind(&self, account_id: &str) -> Result<String, GatewayError> {
        let connectors = self.lock_connectors()?;
        let status = connectors.status_for_account(account_id)?;
        Ok(status.kind.as_str().to_owned())
    }

    /// Verifies that an account is connected and records readiness transitions.
    ///
    /// The registry lock is intentionally held for the whole status check and
    /// resilience update so the read-then-update sequence stays consistent; the
    /// per-client write taken by `note_account_reconnect_success` runs while the
    /// registry lock is held. This is acceptable because the operation is
    /// in-memory bookkeeping (no network I/O) and the reset is skipped entirely
    /// when there is no resilience state to clear.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired, [`GatewayError::UnknownAccount`] when the account is
    /// not registered, or [`GatewayError::ConnectorNotReady`] when the account
    /// is currently disconnected.
    pub fn ensure_account_ready(&self, account_id: &str) -> Result<(), GatewayError> {
        let connectors = self.lock_connectors()?;

        let status = connectors.status_for_account(account_id)?;
        if let Some(throttled_until_ms) = status.resilience_state.throttled_until_ms
            && unix_now_ms() < throttled_until_ms
        {
            return Err(GatewayError::ConnectorNotReady {
                account_id: account_id.to_owned(),
                state: status.state,
                reason: format!("provider rate limit active until {throttled_until_ms}"),
            });
        }
        if matches!(status.state, ConnectionState::Connected) {
            // Only reset resilience state when there is something to reset:
            // the reset takes the per-client write lock, which would otherwise
            // queue behind in-flight fetches on every healthy call.
            if status.resilience_state.consecutive_failures > 0
                || status.resilience_state.next_reconnect_at_ms.is_some()
                || status.resilience_state.throttled_until_ms.is_some()
            {
                // Best-effort bookkeeping: the account was just observed via
                // `status_for_account`, so a failure here is unexpected. Log it
                // rather than discarding it silently so a stale resilience state
                // is traceable.
                if let Err(error) = connectors.note_account_reconnect_success(account_id) {
                    tracing::warn!(
                        account_id,
                        %error,
                        "failed to record connector reconnect success; resilience state may be stale",
                    );
                }
            }
            return Ok(());
        }

        // Best-effort bookkeeping before reporting the not-ready status. A
        // failure here means the subsequent `status_for_account` read could
        // return state that predates this disconnect, so surface it via a log
        // instead of swallowing it. (If the account vanished concurrently the
        // `status_for_account` call below propagates an `UnknownAccount` error.)
        if let Err(error) = connectors.note_account_disconnect(account_id, unix_now_ms()) {
            tracing::warn!(
                account_id,
                %error,
                "failed to record connector disconnect; readiness reason may be stale",
            );
        }
        let status = connectors.status_for_account(account_id)?;
        Err(GatewayError::ConnectorNotReady {
            account_id: account_id.to_owned(),
            state: status.state,
            reason: readiness_reason(&status),
        })
    }

    /// Fetches a connector account snapshot without a readiness pre-check.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired, [`GatewayError::UnknownAccount`] when the account is
    /// not registered, or connector-specific fetch failures.
    pub fn fetch_account_snapshot_unchecked(
        &self,
        account_id: &str,
    ) -> Result<ConnectorAccountSnapshot, GatewayError> {
        let client = self.client_unchecked(account_id)?;
        self.track_rate_limit(account_id, client.fetch_account_snapshot())
    }

    /// Normalizes a provider market-stream payload into a runtime bar update.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific normalization error.
    pub fn normalize_market_stream_payload(
        &self,
        account_id: &str,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        client
            .normalize_market_data_event(payload)
            .map_err(Into::into)
    }

    /// Starts a long-lived preview stream session for a connector account.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific setup error.
    pub fn start_preview_stream_session(
        &self,
        account_id: &str,
    ) -> Result<Option<ConnectorPreviewStreamSession>, GatewayError> {
        let client = self.client_unchecked(account_id)?;
        client.start_preview_stream_session().map_err(Into::into)
    }

    /// Fetches the latest bar for a symbol and timeframe.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific market-data error.
    pub fn fetch_latest_bar(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        self.track_rate_limit(account_id, client.fetch_latest_bar(symbol, timeframe))
    }

    /// Fetches recent bars for a symbol and timeframe.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific market-data error.
    pub fn fetch_recent_bars(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        self.track_rate_limit(
            account_id,
            client.fetch_recent_bars(symbol, timeframe, limit),
        )
    }

    /// Fetches the latest confirmed bar timestamp for a symbol and timeframe.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific market-data error.
    pub fn fetch_latest_confirmed_bar_timestamp(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        self.track_rate_limit(
            account_id,
            client.fetch_latest_confirmed_bar_timestamp(symbol, timeframe),
        )
    }

    /// Fetches a page of confirmed bars in `(start_after, end_at]` for a symbol and timeframe.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific market-data error.
    pub fn fetch_confirmed_bars_range(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<DateTime<Utc>>,
        end_at: DateTime<Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        self.track_rate_limit(
            account_id,
            client.fetch_confirmed_bars_range(symbol, timeframe, start_after, end_at, limit),
        )
    }

    /// Fetches symbol-level execution constraints without a readiness pre-check.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired, [`GatewayError::UnknownAccount`] when the account is
    /// not registered, or a connector-specific symbol-constraints error.
    pub fn fetch_symbol_constraints_unchecked(
        &self,
        account_id: &str,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, GatewayError> {
        let client = self.client_unchecked(account_id)?;
        self.track_rate_limit(account_id, client.fetch_symbol_constraints(symbol))
    }

    /// Fetches symbol constraints and normalizes them into the shared
    /// execution-constraint shape.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::LockPoisoned`] when the connector registry mutex
    /// cannot be acquired, [`GatewayError::UnknownAccount`] when the account is
    /// not registered, or a connector-specific symbol-constraints error.
    pub fn fetch_normalized_symbol_constraints_unchecked(
        &self,
        account_id: &str,
        symbol: &str,
    ) -> Result<NormalizedSymbolConstraints, GatewayError> {
        let connector_constraints = self.fetch_symbol_constraints_unchecked(account_id, symbol)?;
        Ok(normalize_symbol_constraints(&connector_constraints))
    }

    /// Submits an execution request through the configured connector account.
    ///
    /// # Errors
    ///
    /// Returns [`GatewayError::ConnectorNotReady`] when the account is not
    /// connected, [`GatewayError::UnknownAccount`] when the account is not
    /// registered, [`GatewayError::LockPoisoned`] when the registry mutex cannot
    /// be acquired, or a connector-specific order-submission error.
    pub fn submit_order(
        &self,
        account_id: &str,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, GatewayError> {
        self.ensure_account_ready(account_id)?;
        let client = self.connected_client(account_id)?;
        self.track_rate_limit(account_id, client.submit_order(request))
    }
}

/// Builds a connector registry from validated runtime account config.
///
/// # Errors
///
/// Returns [`GatewayError::UnsupportedConnectorKind`] when an account refers to
/// an unknown connector kind, or connector-specific initialization failures
/// when the registry cannot be created.
pub fn build_connector_registry(
    accounts: &[AccountConfig],
) -> Result<ConnectorRegistry, GatewayError> {
    let connector_accounts = accounts
        .iter()
        .map(|account| {
            let kind = ConnectorKind::parse(account.kind.as_str()).ok_or_else(|| {
                GatewayError::UnsupportedConnectorKind {
                    account_id: account.id.clone(),
                    kind: account.kind.clone(),
                }
            })?;

            Ok(openticker_connectors::ConnectorAccount {
                account_id: account.id.clone(),
                kind,
                mode: account.mode,
                use_demo_mode: account.use_demo_mode,
                api_key_env: account.api_key_env.clone(),
                api_secret_env: account.api_secret_env.clone(),
                passphrase_env: account.passphrase_env.clone(),
                reconciliation_remote_snapshot: account.reconciliation_remote_snapshot,
                execution_remote_submission: account.execution_remote_submission_enabled(),
                reconciliation_base_url: account.reconciliation_base_url.clone(),
            })
        })
        .collect::<Result<Vec<_>, GatewayError>>()?;

    ConnectorRegistry::from_accounts(connector_accounts).map_err(Into::into)
}

fn readiness_reason(status: &ConnectorAccountStatus) -> String {
    let mut reason = status.message.clone();
    if status.resilience_state.consecutive_failures > 0 {
        let _ = write!(
            reason,
            ",consecutive_failures={}",
            status.resilience_state.consecutive_failures
        );
    }
    if let Some(next_reconnect_at_ms) = status.resilience_state.next_reconnect_at_ms {
        let _ = write!(reason, ",next_reconnect_at_ms={next_reconnect_at_ms}");
    }
    if let Some(throttled_until_ms) = status.resilience_state.throttled_until_ms {
        let _ = write!(reason, ",throttled_until_ms={throttled_until_ms}");
    }
    reason
}

#[must_use]
pub fn execution_constraints_are_complete(constraints: &ExecutionConstraintsConfig) -> bool {
    constraints.quantity_step.is_some()
        && constraints.min_quantity.is_some()
        && constraints.min_notional_usd.is_some()
}

#[must_use]
pub fn resolve_effective_execution_constraints(
    configured_constraints: &ExecutionConstraintsConfig,
    connector_constraints: Option<&ExecutionConstraintsConfig>,
) -> ExecutionConstraintsConfig {
    ExecutionConstraintsConfig {
        quantity_step: configured_constraints
            .quantity_step
            .or_else(|| connector_constraints.and_then(|constraints| constraints.quantity_step)),
        min_quantity: configured_constraints
            .min_quantity
            .or_else(|| connector_constraints.and_then(|constraints| constraints.min_quantity)),
        min_notional_usd: configured_constraints
            .min_notional_usd
            .or_else(|| connector_constraints.and_then(|constraints| constraints.min_notional_usd)),
    }
}

#[must_use]
pub fn normalize_symbol_constraints(
    connector_constraints: &ConnectorSymbolConstraints,
) -> NormalizedSymbolConstraints {
    NormalizedSymbolConstraints {
        execution_constraints: ExecutionConstraintsConfig {
            quantity_step: sanitize_positive_constraint(connector_constraints.quantity_step),
            min_quantity: sanitize_positive_constraint(connector_constraints.min_quantity),
            min_notional_usd: sanitize_positive_constraint(connector_constraints.min_notional_usd),
        },
        fractional_entry_supported: connector_constraints.fractional_entry_supported,
        source: connector_constraints.source.clone(),
    }
}

fn sanitize_positive_constraint(value: Option<f64>) -> Option<f64> {
    value.filter(|candidate| candidate.is_finite() && *candidate > 0.0)
}

fn unix_now_ms() -> i64 {
    let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return 0;
    };
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        AccountConfig, Arc, ConnectorSymbolConstraints, Gateway, GatewayError, Mutex,
        build_connector_registry, execution_constraints_are_complete, normalize_symbol_constraints,
        resolve_effective_execution_constraints,
    };
    use openticker_config::ExecutionConstraintsConfig;
    use openticker_core::ExecutionMode;

    fn account(kind: &str) -> AccountConfig {
        AccountConfig {
            id: "paper-account".to_owned(),
            kind: kind.to_owned(),
            mode: ExecutionMode::Paper,
            api_key_env: None,
            api_secret_env: None,
            passphrase_env: None,
            use_demo_mode: false,
            reconciliation_remote_snapshot: false,
            execution_remote_submission: None,
            reconciliation_base_url: None,
            cash_balance_assets: Vec::new(),
            total_budget_usd: 10_000.0,
        }
    }

    #[test]
    fn normalize_symbol_constraints_filters_non_positive_values() {
        let normalized = normalize_symbol_constraints(&ConnectorSymbolConstraints {
            fractional_entry_supported: Some(true),
            quantity_step: Some(0.0),
            min_quantity: Some(-1.0),
            min_notional_usd: Some(5.0),
            source: Some("connector".to_owned()),
        });

        assert_eq!(normalized.execution_constraints.quantity_step, None);
        assert_eq!(normalized.execution_constraints.min_quantity, None);
        assert_eq!(normalized.execution_constraints.min_notional_usd, Some(5.0));
        assert_eq!(normalized.fractional_entry_supported, Some(true));
        assert_eq!(normalized.source.as_deref(), Some("connector"));
        assert!(normalized.has_numeric_constraints());
    }

    #[test]
    fn resolve_effective_execution_constraints_prefers_instance_over_connector_values() {
        let configured = ExecutionConstraintsConfig {
            quantity_step: Some(0.01),
            min_quantity: None,
            min_notional_usd: Some(15.0),
        };
        let connector = ExecutionConstraintsConfig {
            quantity_step: Some(0.1),
            min_quantity: Some(0.001),
            min_notional_usd: Some(5.0),
        };

        let resolved = resolve_effective_execution_constraints(&configured, Some(&connector));
        assert_eq!(resolved.quantity_step, Some(0.01));
        assert_eq!(resolved.min_quantity, Some(0.001));
        assert_eq!(resolved.min_notional_usd, Some(15.0));
    }

    #[test]
    fn execution_constraints_are_complete_requires_all_numeric_fields() {
        assert!(!execution_constraints_are_complete(
            &ExecutionConstraintsConfig {
                quantity_step: Some(0.01),
                min_quantity: Some(1.0),
                min_notional_usd: None,
            }
        ));
        assert!(execution_constraints_are_complete(
            &ExecutionConstraintsConfig {
                quantity_step: Some(0.01),
                min_quantity: Some(1.0),
                min_notional_usd: Some(5.0),
            }
        ));
    }

    #[test]
    fn build_connector_registry_accepts_supported_account_kinds() {
        let registry = build_connector_registry(&[account("alpaca")]).unwrap();
        let gateway = Gateway::new(Arc::new(Mutex::new(registry)));

        let statuses = gateway.statuses().unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].account_id, "paper-account");
        assert_eq!(statuses[0].kind.as_str(), "alpaca");
    }

    #[test]
    fn build_connector_registry_rejects_unsupported_account_kinds() {
        let error = build_connector_registry(&[account("not-a-real-connector")]).unwrap_err();

        assert!(matches!(
            error,
            GatewayError::UnsupportedConnectorKind {
                account_id,
                kind,
            } if account_id == "paper-account" && kind == "not-a-real-connector"
        ));
    }
}
