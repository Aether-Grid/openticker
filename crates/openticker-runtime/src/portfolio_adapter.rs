use crate::repo::RuntimeRepo;
use crate::{OhlcvBar, OrderLedgerOutcome, ServiceError, TradeIntent};

pub(crate) struct PortfolioAdapter<'a> {
    repo: RuntimeRepo<'a>,
}

impl<'a> PortfolioAdapter<'a> {
    pub(crate) fn from_parts(repo: RuntimeRepo<'a>) -> PortfolioAdapter<'a> {
        Self { repo }
    }
}

impl PortfolioAdapter<'_> {
    pub(crate) fn record_ledger_rejection(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: OrderLedgerOutcome,
    ) -> Result<(), ServiceError> {
        self.record_ledger_rejection_with_trace(
            instance_id,
            account_id,
            symbol,
            bar,
            intent,
            ledger_outcome,
            None,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_ledger_rejection_with_trace(
        &mut self,
        instance_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: OrderLedgerOutcome,
        trace_id: Option<&str>,
    ) -> Result<serde_json::Value, ServiceError> {
        let payload = self.repo.as_read().ledger_rejection_event_payload(
            instance_id,
            account_id,
            symbol,
            bar,
            intent,
            ledger_outcome,
        )?;

        self.repo.state.observability.risk_rejects_total = self
            .repo
            .state
            .observability
            .risk_rejects_total
            .saturating_add(1);
        match ledger_outcome {
            OrderLedgerOutcome::BotExhausted => {
                self.repo.state.observability.ledger_bot_rejects_total = self
                    .repo
                    .state
                    .observability
                    .ledger_bot_rejects_total
                    .saturating_add(1);
            }
            OrderLedgerOutcome::AccountExhausted => {
                self.repo.state.observability.ledger_account_rejects_total = self
                    .repo
                    .state
                    .observability
                    .ledger_account_rejects_total
                    .saturating_add(1);
            }
            OrderLedgerOutcome::Dust => {}
        }

        self.repo.as_read().append_runtime_event_with_trace(
            "risk",
            Some(instance_id),
            "risk.rejected",
            payload.to_string(),
            trace_id,
        )?;

        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ReservationError;
    use crate::Runtime;
    use crate::test_support::fixture_bundle;
    use openticker_ledger::{AccountLedger, LedgerException, LedgerExceptionKind, LedgerOwnerPath};

    #[test]
    fn account_capital_ledger_rejects_bot_and_account_caps() {
        let mut ledger = AccountLedger::new(1_000.0);
        let bot_a = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        assert!(ledger.try_reserve_open(&bot_a, 400.0, 40.0).is_ok());
        assert!(matches!(
            ledger.try_reserve_open(&bot_a, 1.0, 40.0),
            Err(ReservationError::BotCapacityExceeded)
        ));

        let mut ledger = AccountLedger::new(1_000.0);
        let bot_a = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let bot_b = LedgerOwnerPath::new("acct", "bot-b", "MSFT");
        let bot_c = LedgerOwnerPath::new("acct", "bot-c", "NVDA");
        assert!(ledger.try_reserve_open(&bot_a, 400.0, 40.0).is_ok());
        assert!(ledger.try_reserve_open(&bot_b, 500.0, 60.0).is_ok());
        assert!(matches!(
            ledger.try_reserve_open(&bot_c, 200.0, 30.0),
            Err(ReservationError::AccountCapacityExceeded)
        ));
    }

    #[test]
    fn account_capital_ledger_uses_lower_live_balance() {
        let mut ledger = AccountLedger::new(1_000.0);
        ledger.set_live_balance_usd(Some(800.0));

        assert!((ledger.effective_cap_usd() - 800.0).abs() < 1e-6);
        assert!((ledger.bot_allocated_usd(25.0) - 200.0).abs() < 1e-6);
    }

    #[test]
    fn account_capital_ledger_reconciles_partial_fill_and_release() {
        let mut ledger = AccountLedger::new(1_000.0);
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        assert!(ledger.try_reserve_open(&owner, 500.0, 100.0).is_ok());
        ledger.reconcile_open_fill(&owner, 200.0, 500.0);
        assert!((ledger.bot_attributed_open_notional_usd("bot-a") - 200.0).abs() < 1e-6);

        ledger.release_position(&owner, 50.0);
        assert!((ledger.bot_attributed_open_notional_usd("bot-a") - 150.0).abs() < 1e-6);
    }

    #[test]
    fn ledger_snapshot_reports_unattributed_open_notional() {
        let config = fixture_bundle();
        let runtime = Runtime::from_config(&config);
        let ledger = runtime
            .repo()
            .account_ledger_handle("alpaca-paper")
            .unwrap();
        let mut ledger = ledger.lock().unwrap();
        ledger.set_unattributed_open_notional_usd(250.0);
        drop(ledger);

        let ledger = runtime.ledger_snapshot();
        let account = ledger
            .accounts
            .iter()
            .find(|account| account.id == "alpaca-paper")
            .unwrap();
        assert!((account.total_committed_notional_usd - 250.0).abs() < 1e-6);
        assert!((account.unattributed_open_notional_usd - 250.0).abs() < 1e-6);
    }

    #[test]
    fn ledger_snapshot_reports_blocking_exception_without_fake_used_notional() {
        let config = fixture_bundle();
        let runtime = Runtime::from_config(&config);
        let ledger = runtime
            .repo()
            .account_ledger_handle("alpaca-paper")
            .unwrap();
        let mut ledger = ledger.lock().unwrap();
        ledger.replace_exceptions(vec![LedgerException {
            kind: LedgerExceptionKind::AmbiguousSymbolOwner,
            owner: None,
            symbol: Some("AAPL".to_owned()),
            detail: "matching_bots=aapl,msft".to_owned(),
            blocks_new_opens: true,
        }]);
        drop(ledger);

        let ledger = runtime.ledger_snapshot();
        let account = ledger
            .accounts
            .iter()
            .find(|account| account.id == "alpaca-paper")
            .unwrap();
        assert!(account.total_committed_notional_usd.abs() < 1e-6);
        assert!(account.tradeable_open_room_usd.abs() < 1e-6);
        assert!((account.blocked_open_room_usd - account.effective_cap_usd).abs() < 1e-6);
        assert_eq!(account.exceptions.len(), 1);
    }

    #[test]
    fn refresh_account_ledger_from_connector_preserves_existing_live_balance_on_failure() {
        let mut config = fixture_bundle();
        config.accounts[0].reconciliation_remote_snapshot = true;
        config.accounts[0].api_key_env = Some("OPENTICKER_TEST_MISSING_KEY".to_owned());
        config.accounts[0].api_secret_env = Some("OPENTICKER_TEST_MISSING_SECRET".to_owned());

        let runtime = Runtime::from_config(&config);
        let ledger = runtime
            .repo()
            .account_ledger_handle("alpaca-paper")
            .unwrap();
        {
            let mut ledger = ledger.lock().unwrap();
            ledger.set_live_balance_usd(Some(800.0));
        }

        runtime
            .refresh_account_ledger_from_connector("alpaca-paper")
            .unwrap();

        let ledger = runtime.ledger_snapshot();
        let account = ledger
            .accounts
            .iter()
            .find(|account| account.id == "alpaca-paper")
            .unwrap();
        assert_eq!(account.live_balance_usd, Some(800.0));
        assert!((account.effective_cap_usd - 800.0).abs() < 1e-6);
    }

    #[test]
    fn record_ledger_rejection_uses_bot_id_for_multi_symbol_lanes() {
        let mut config = fixture_bundle();
        config.instances[0].symbols = vec!["AAPL".to_owned(), "MSFT".to_owned()];
        config.instances[0].budget.pct = 50.0;

        let mut runtime = Runtime::from_config(&config);
        let aapl_lane = runtime
            .resolve_lane_id("aapl", "AAPL")
            .expect("AAPL lane should resolve");

        {
            let ledger = runtime
                .repo()
                .account_ledger_handle("alpaca-paper")
                .expect("account ledger should exist");
            let mut ledger = ledger.lock().expect("account ledger lock should succeed");
            let owner = LedgerOwnerPath::new("alpaca-paper", "aapl", "AAPL");
            ledger
                .try_reserve_open(&owner, 4_000.0, 50.0)
                .expect("reservation should fit bot allocation");
        }

        {
            let mut adapter = PortfolioAdapter::from_parts(runtime.repo_mut());
            adapter
                .record_ledger_rejection(
                    &aapl_lane,
                    "alpaca-paper",
                    "AAPL",
                    &crate::test_support::test_bar_at("2030-01-01T00:00:00Z", 100.0),
                    TradeIntent::OpenLong,
                    OrderLedgerOutcome::BotExhausted,
                )
                .expect("ledger rejection event should record");
        }

        let risk_events = runtime
            .recent_events_by_scope_and_entity("risk", &aapl_lane, 10)
            .expect("risk events should load");
        let payload = serde_json::from_str::<serde_json::Value>(
            &risk_events
                .last()
                .expect("risk rejection event should exist")
                .payload,
        )
        .expect("risk payload should be valid JSON");

        let committed = payload["committed_usd"]
            .as_f64()
            .expect("committed_usd should be present");
        let tradeable_room = payload["tradeable_room_usd"]
            .as_f64()
            .expect("tradeable_room_usd should be present");

        assert_eq!(payload["symbol"], "AAPL");
        assert_eq!(payload["bar_timestamp"], "2030-01-01T00:00:00+00:00");
        assert!((committed - 4_000.0).abs() < 1e-6);
        assert!((tradeable_room - 6_000.0).abs() < 1e-6);
    }
}
