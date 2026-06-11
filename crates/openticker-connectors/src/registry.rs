use crate::error::ConnectorError;
use crate::traits::ConnectorClient;
use crate::types::{
    ConfirmedBarPage, ConnectionState, ConnectorAccount, ConnectorAccountSnapshot,
    ConnectorAccountStatus, ConnectorKind, ConnectorPreviewStreamSession,
    ConnectorPrivateStreamEvent, ConnectorSymbolConstraints,
};
use crate::{alpaca, binance};
use chrono::{DateTime, Utc};
use openticker_core::{OhlcvBar, Timeframe};
use openticker_data::NormalizedBarUpdate;
use openticker_execution::{AcceptedOrder, ExecutionRequest};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

type SharedConnectorClient = Arc<RwLock<Box<dyn ConnectorClient>>>;

/// Recovers a poisoned connector-client read lock, logging the event.
///
/// Workspace policy is to recover from poisoning and log rather than fail
/// forever: a connector thread that panicked while holding the lock leaves the
/// resilience state structurally valid (plain in-memory fields), but the panic
/// must be observable instead of silently swallowed by `into_inner()`.
fn read_recover_poison(
    lock: &RwLock<Box<dyn ConnectorClient>>,
) -> RwLockReadGuard<'_, Box<dyn ConnectorClient>> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("connector client lock was poisoned; recovering for read access");
            lock.clear_poison();
            poisoned.into_inner()
        }
    }
}

/// Recovers a poisoned connector-client write lock, logging the event.
///
/// See [`read_recover_poison`] for the recovery rationale.
fn write_recover_poison(
    lock: &RwLock<Box<dyn ConnectorClient>>,
) -> RwLockWriteGuard<'_, Box<dyn ConnectorClient>> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!("connector client lock was poisoned; recovering for write access");
            lock.clear_poison();
            poisoned.into_inner()
        }
    }
}

#[derive(Debug)]
struct ConnectorRegistryEntry {
    account: ConnectorAccount,
    validation_error: Option<String>,
    client: SharedConnectorClient,
}

/// Shared handle to a single account's connector client.
///
/// The handle is detached from the registry, so callers can run network
/// operations without holding the registry lock: a slow round-trip for one
/// account never serializes other accounts' fetches or status queries.
/// Concurrent operations on the same client run in parallel (read lock);
/// only resilience-state mutations (`note_*`) take the write lock.
#[derive(Debug, Clone)]
pub struct ConnectorClientHandle {
    client: SharedConnectorClient,
}

impl ConnectorClientHandle {
    fn read(&self) -> RwLockReadGuard<'_, Box<dyn ConnectorClient>> {
        read_recover_poison(&self.client)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when snapshot retrieval fails.
    pub fn fetch_account_snapshot(&self) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        self.read().fetch_account_snapshot()
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when symbol metadata retrieval fails.
    pub fn fetch_symbol_constraints(
        &self,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        self.read().fetch_symbol_constraints(symbol)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when bar retrieval fails.
    pub fn fetch_latest_bar(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError> {
        self.read().fetch_latest_bar(symbol, timeframe)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when historical bar retrieval fails.
    pub fn fetch_recent_bars(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError> {
        self.read().fetch_recent_bars(symbol, timeframe, limit)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when confirmed-target retrieval fails.
    pub fn fetch_latest_confirmed_bar_timestamp(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, ConnectorError> {
        self.read()
            .fetch_latest_confirmed_bar_timestamp(symbol, timeframe)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when confirmed-range retrieval fails.
    pub fn fetch_confirmed_bars_range(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<DateTime<Utc>>,
        end_at: DateTime<Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ConnectorError> {
        self.read()
            .fetch_confirmed_bars_range(symbol, timeframe, start_after, end_at, limit)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when order submission fails.
    pub fn submit_order(
        &self,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, ConnectorError> {
        self.read().submit_order(request)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when payload normalization fails.
    pub fn normalize_market_data_event(
        &self,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
        self.read().normalize_market_data_event(payload)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when session creation fails.
    pub fn start_preview_stream_session(
        &self,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError> {
        self.read().start_preview_stream_session()
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when payload normalization fails.
    pub fn normalize_private_stream_event(
        &self,
        payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
        self.read().normalize_private_stream_event(payload)
    }
}

#[derive(Debug, Default)]
pub struct ConnectorRegistry {
    entries: HashMap<String, ConnectorRegistryEntry>,
}

impl ConnectorRegistry {
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when account entries are invalid or duplicated.
    pub fn from_accounts(accounts: Vec<ConnectorAccount>) -> Result<Self, ConnectorError> {
        let mut entries = HashMap::new();
        for account in accounts {
            if entries.contains_key(account.account_id.as_str()) {
                return Err(ConnectorError::DuplicateAccountId {
                    account_id: account.account_id,
                });
            }

            let (client, validation_error): (Box<dyn ConnectorClient>, Option<String>) =
                match account.kind {
                    ConnectorKind::Alpaca => {
                        let connector = alpaca::AlpacaConnector::new(&account);
                        let validation_error = connector
                            .validate_mode()
                            .err()
                            .map(|error| error.to_string());
                        (Box::new(connector), validation_error)
                    }
                    ConnectorKind::Binance => {
                        let connector = binance::BinanceConnector::new(&account);
                        let validation_error = connector
                            .validate_mode()
                            .err()
                            .map(|error| error.to_string());
                        (Box::new(connector), validation_error)
                    }
                };

            entries.insert(
                account.account_id.clone(),
                ConnectorRegistryEntry {
                    account,
                    validation_error,
                    client: Arc::new(RwLock::new(client)),
                },
            );
        }

        Ok(Self { entries })
    }

    #[must_use]
    pub fn statuses(&self) -> Vec<ConnectorAccountStatus> {
        let mut statuses = self
            .entries
            .values()
            .map(Self::status_from_entry)
            .collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.account_id.cmp(&right.account_id));
        statuses
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account ID is unknown.
    pub fn status_for_account(
        &self,
        account_id: &str,
    ) -> Result<ConnectorAccountStatus, ConnectorError> {
        let entry = self
            .entries
            .get(account_id)
            .ok_or_else(|| ConnectorError::UnknownAccount {
                account_id: account_id.to_owned(),
            })?;
        Ok(Self::status_from_entry(entry))
    }

    /// Returns a detached client handle after verifying the account is
    /// connected, so network operations can run without the registry lock.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown or unavailable.
    pub fn connected_client(
        &self,
        account_id: &str,
    ) -> Result<ConnectorClientHandle, ConnectorError> {
        let status = self.status_for_account(account_id)?;
        if !matches!(status.state, ConnectionState::Connected) {
            return Err(ConnectorError::Unavailable {
                kind: status.kind,
                state: status.state,
            });
        }
        self.client_unchecked(account_id)
    }

    /// Returns a detached client handle without a connection-state pre-check.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown.
    pub fn client_unchecked(
        &self,
        account_id: &str,
    ) -> Result<ConnectorClientHandle, ConnectorError> {
        let entry = self
            .entries
            .get(account_id)
            .ok_or_else(|| ConnectorError::UnknownAccount {
                account_id: account_id.to_owned(),
            })?;
        Ok(ConnectorClientHandle {
            client: Arc::clone(&entry.client),
        })
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or snapshot retrieval fails.
    pub fn fetch_account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        self.connected_client(account_id)?.fetch_account_snapshot()
    }

    /// Fetches an account snapshot without requiring the connector health state to be
    /// [`ConnectionState::Connected`].
    ///
    /// This is intended for reconciliation flows where a connector stream may be degraded
    /// while REST account endpoints are still reachable.
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown or snapshot retrieval fails.
    pub fn fetch_account_snapshot_unchecked(
        &self,
        account_id: &str,
    ) -> Result<ConnectorAccountSnapshot, ConnectorError> {
        self.client_unchecked(account_id)?.fetch_account_snapshot()
    }

    /// Fetches per-symbol execution constraints without requiring the connector
    /// health state to be [`ConnectionState::Connected`].
    ///
    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown or symbol metadata retrieval fails.
    pub fn fetch_symbol_constraints_unchecked(
        &self,
        account_id: &str,
        symbol: &str,
    ) -> Result<ConnectorSymbolConstraints, ConnectorError> {
        self.client_unchecked(account_id)?
            .fetch_symbol_constraints(symbol)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or bar retrieval fails.
    pub fn fetch_latest_bar(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<OhlcvBar, ConnectorError> {
        self.connected_client(account_id)?
            .fetch_latest_bar(symbol, timeframe)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or historical bar retrieval fails.
    pub fn fetch_recent_bars(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
        limit: usize,
    ) -> Result<Vec<OhlcvBar>, ConnectorError> {
        self.connected_client(account_id)?
            .fetch_recent_bars(symbol, timeframe, limit)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or confirmed-target retrieval fails.
    pub fn fetch_latest_confirmed_bar_timestamp(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
    ) -> Result<Option<DateTime<Utc>>, ConnectorError> {
        self.connected_client(account_id)?
            .fetch_latest_confirmed_bar_timestamp(symbol, timeframe)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or confirmed-range retrieval fails.
    pub fn fetch_confirmed_bars_range(
        &self,
        account_id: &str,
        symbol: &str,
        timeframe: Timeframe,
        start_after: Option<DateTime<Utc>>,
        end_at: DateTime<Utc>,
        limit: usize,
    ) -> Result<ConfirmedBarPage, ConnectorError> {
        self.connected_client(account_id)?
            .fetch_confirmed_bars_range(symbol, timeframe, start_after, end_at, limit)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or order submission fails.
    pub fn submit_order(
        &self,
        account_id: &str,
        request: &ExecutionRequest,
    ) -> Result<AcceptedOrder, ConnectorError> {
        self.connected_client(account_id)?.submit_order(request)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or payload normalization fails.
    pub fn normalize_market_data_event(
        &self,
        account_id: &str,
        payload: &str,
    ) -> Result<Option<NormalizedBarUpdate>, ConnectorError> {
        self.connected_client(account_id)?
            .normalize_market_data_event(payload)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown or session creation fails.
    pub fn start_preview_stream_session(
        &self,
        account_id: &str,
    ) -> Result<Option<ConnectorPreviewStreamSession>, ConnectorError> {
        self.client_unchecked(account_id)?
            .start_preview_stream_session()
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account is unknown, unavailable, or payload normalization fails.
    pub fn normalize_private_stream_event(
        &self,
        account_id: &str,
        payload: &str,
    ) -> Result<Option<ConnectorPrivateStreamEvent>, ConnectorError> {
        self.connected_client(account_id)?
            .normalize_private_stream_event(payload)
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account ID is unknown.
    pub fn note_account_disconnect(
        &self,
        account_id: &str,
        now_ms: i64,
    ) -> Result<(), ConnectorError> {
        let entry = self
            .entries
            .get(account_id)
            .ok_or_else(|| ConnectorError::UnknownAccount {
                account_id: account_id.to_owned(),
            })?;
        write_recover_poison(&entry.client).note_disconnect(now_ms);
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account ID is unknown.
    pub fn note_account_reconnect_success(&self, account_id: &str) -> Result<(), ConnectorError> {
        let entry = self
            .entries
            .get(account_id)
            .ok_or_else(|| ConnectorError::UnknownAccount {
                account_id: account_id.to_owned(),
            })?;
        write_recover_poison(&entry.client).note_reconnect_success();
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`ConnectorError`] when the account ID is unknown.
    pub fn note_account_rate_limit(
        &self,
        account_id: &str,
        now_ms: i64,
        throttle_window_ms: u64,
    ) -> Result<(), ConnectorError> {
        let entry = self
            .entries
            .get(account_id)
            .ok_or_else(|| ConnectorError::UnknownAccount {
                account_id: account_id.to_owned(),
            })?;
        write_recover_poison(&entry.client).note_rate_limit(now_ms, throttle_window_ms);
        Ok(())
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.entries.values().all(|entry| {
            matches!(
                Self::status_from_entry(entry).state,
                ConnectionState::Connected
            )
        })
    }

    fn status_from_entry(entry: &ConnectorRegistryEntry) -> ConnectorAccountStatus {
        let client = read_recover_poison(&entry.client);
        let mut health = client.health();
        let resilience_policy = client.resilience_policy();
        let resilience_state = client.resilience_state();
        drop(client);
        if entry.account.use_demo_mode {
            health.message = format!("{},demo=true", health.message);
        }
        if let Some(error) = entry.validation_error.as_deref() {
            health.state = ConnectionState::Degraded;
            health.message = format!("{},{}", health.message, error);
        }

        ConnectorAccountStatus {
            account_id: entry.account.account_id.clone(),
            kind: entry.account.kind,
            mode: entry.account.mode,
            state: health.state,
            message: health.message,
            resilience_policy,
            resilience_state,
        }
    }
}
