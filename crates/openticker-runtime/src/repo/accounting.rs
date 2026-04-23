use super::RuntimeRepoRead;
use crate::{
    AccountPortfolioSnapshot, AccountRiskSnapshot, BotPortfolioSnapshot, ConnectorAccountSnapshot,
    LanePortfolioSnapshot, LaneRuntime, LedgerPortfolioSnapshot, LedgerRooms, OhlcvBar,
    OrderLedgerOutcome, PositionRecord, RECONCILIATION_JOURNAL_SCAN_LIMIT, Runtime, ServiceError,
    TradeIntent, current_instance_open_notional_usd, effective_position_quantity,
    ledger_outcome_reason_code, trade_intent_label,
};
use openticker_portfolio::{LatestLanePosition, PortfolioLaneView};
use tracing::warn;

#[derive(Debug)]
pub(crate) struct PortfolioSnapshotParts {
    pub(crate) accounts: Vec<AccountPortfolioSnapshot>,
    pub(crate) bots: Vec<BotPortfolioSnapshot>,
    pub(crate) lanes: Vec<LanePortfolioSnapshot>,
}

#[derive(Debug)]
pub(crate) struct AccountLedgerRefreshInputs {
    pub(crate) account_kind: String,
    pub(crate) cash_balance_assets: Vec<String>,
    pub(crate) reconciliation_remote_snapshot: bool,
    pub(crate) account_lanes: Vec<PortfolioLaneView>,
    pub(crate) latest_positions: Vec<LatestLanePosition>,
    pub(crate) reconciliation_symbols: Vec<String>,
}

fn portfolio_lane_view_from_runtime(lane_id: &str, runtime: &LaneRuntime) -> PortfolioLaneView {
    PortfolioLaneView {
        lane_id: lane_id.to_owned(),
        bot_id: runtime.config.id.clone(),
        account_id: runtime.config.account.clone(),
        symbol: runtime.lane_symbol.clone(),
        budget_pct: runtime.config.budget.pct,
        effective_position_quantity: effective_position_quantity(runtime),
        position_notional_usd: current_instance_open_notional_usd(runtime),
        daily_loss_pct_accumulated: runtime.daily_loss_pct_accumulated,
    }
}

impl RuntimeRepoRead<'_> {
    pub(crate) fn accounting_lane(&self, lane_id: &str) -> Result<PortfolioLaneView, ServiceError> {
        let runtime = self.instance(lane_id)?;
        Ok(portfolio_lane_view_from_runtime(lane_id, runtime))
    }

    pub(crate) fn accounting_lanes_for_account(&self, account_id: &str) -> Vec<PortfolioLaneView> {
        let mut lanes = self
            .state
            .lanes
            .iter()
            .filter(|(_, runtime)| runtime.config.account == account_id)
            .map(|(lane_id, runtime)| portfolio_lane_view_from_runtime(lane_id, runtime))
            .collect::<Vec<_>>();
        lanes.sort_by(|left, right| left.lane_id.cmp(&right.lane_id));
        lanes
    }

    pub(crate) fn latest_authoritative_position_for_lane(
        &self,
        lane_id: &str,
        limit: usize,
    ) -> Result<Option<PositionRecord>, ServiceError> {
        let lane = self.accounting_lane(lane_id)?;
        let positions = self.recent_positions_for_bot(&lane.bot_id, limit)?;

        Ok(openticker_portfolio::latest_authoritative_position(
            lane.symbol.as_str(),
            &positions,
        ))
    }

    pub(crate) fn latest_authoritative_position_for_instance(
        &self,
        instance_id: &str,
    ) -> Result<Option<PositionRecord>, ServiceError> {
        self.latest_authoritative_position_for_lane(instance_id, RECONCILIATION_JOURNAL_SCAN_LIMIT)
    }

    pub(crate) fn latest_accounting_positions_for_account(
        &self,
        account_id: &str,
        limit: usize,
    ) -> Result<Vec<LatestLanePosition>, ServiceError> {
        self.accounting_lanes_for_account(account_id)
            .into_iter()
            .map(|lane| {
                Ok(LatestLanePosition {
                    lane_id: lane.lane_id.clone(),
                    position: self
                        .latest_authoritative_position_for_lane(lane.lane_id.as_str(), limit)?,
                })
            })
            .collect()
    }

    pub(crate) fn account_ledger_refresh_inputs(
        &self,
        account_id: &str,
        limit: usize,
    ) -> Result<AccountLedgerRefreshInputs, ServiceError> {
        let account = self.catalog.accounts.get(account_id).ok_or_else(|| {
            ServiceError::InvalidConfiguration(format!("missing account config for `{account_id}`"))
        })?;
        let account_lanes = self.accounting_lanes_for_account(account_id);
        let latest_positions = self.latest_accounting_positions_for_account(account_id, limit)?;
        let mut reconciliation_symbols = account_lanes
            .iter()
            .map(|lane| lane.symbol.clone())
            .collect::<Vec<_>>();
        reconciliation_symbols.sort();
        reconciliation_symbols.dedup();

        Ok(AccountLedgerRefreshInputs {
            account_kind: account.kind.clone(),
            cash_balance_assets: account.cash_balance_assets.clone(),
            reconciliation_remote_snapshot: account.reconciliation_remote_snapshot,
            account_lanes,
            latest_positions,
            reconciliation_symbols,
        })
    }

    pub(crate) fn portfolio_snapshot_parts(&self) -> PortfolioSnapshotParts {
        let summaries = self.list_instances();
        let mut bots_by_account =
            std::collections::HashMap::<&str, Vec<&crate::InstanceSummary>>::new();
        for summary in &summaries {
            bots_by_account
                .entry(summary.account.as_str())
                .or_default()
                .push(summary);
        }

        let mut accounts = Vec::new();
        let mut bots = Vec::new();
        let mut lanes = Vec::new();

        for account in self.catalog.accounts.values() {
            let Ok(ledger_handle) = self.account_ledger_handle(&account.id) else {
                continue;
            };
            let Ok(ledger) = ledger_handle.lock() else {
                continue;
            };

            accounts.push(ledger.account_snapshot(&account.id));

            if let Some(account_bots) = bots_by_account.get(account.id.as_str()) {
                for summary in account_bots {
                    let Ok(lanes_for_bot) = self.lanes_for_bot(&summary.id) else {
                        continue;
                    };
                    let Some(primary_lane) = lanes_for_bot.into_iter().next() else {
                        continue;
                    };
                    bots.push(ledger.bot_snapshot(
                        &summary.account,
                        &summary.id,
                        primary_lane.config.budget.pct,
                    ));
                }
            }

            lanes.extend(ledger.lane_snapshots());
        }

        PortfolioSnapshotParts {
            accounts,
            bots,
            lanes,
        }
    }

    #[must_use]
    pub(crate) fn ledger_snapshot(&self) -> LedgerPortfolioSnapshot {
        let parts = self.portfolio_snapshot_parts();
        openticker_portfolio::ledger_snapshot(parts.accounts, parts.bots, parts.lanes)
    }

    pub(crate) fn sync_all_account_ledgers_from_instances(&self) -> Result<(), ServiceError> {
        for account_id in self.catalog.account_ledgers.keys() {
            self.sync_account_ledger_from_instances(account_id.as_str())?;
        }
        Ok(())
    }

    pub(crate) fn sync_account_ledger_from_instances(
        &self,
        account_id: &str,
    ) -> Result<(), ServiceError> {
        let account_lanes = self.accounting_lanes_for_account(account_id);

        let ledger = self.account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        openticker_portfolio::sync_account_ledger_from_lanes(&mut ledger, &account_lanes);
        Ok(())
    }

    pub(crate) fn refresh_account_ledger_from_snapshot(
        &self,
        account_id: &str,
        snapshot: &ConnectorAccountSnapshot,
    ) -> Result<(), ServiceError> {
        let refresh_inputs =
            self.account_ledger_refresh_inputs(account_id, RECONCILIATION_JOURNAL_SCAN_LIMIT)?;
        let ledger = self.account_ledger_handle(account_id)?;
        let mut ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;

        let known_open_notional_usd = if refresh_inputs.account_kind == "binance" {
            ledger.total_attributed_open_notional_usd()
        } else {
            0.0
        };
        let mut refresh_state = openticker_portfolio::account_ledger_refresh_state(
            account_id,
            refresh_inputs.account_kind.as_str(),
            snapshot,
            &refresh_inputs.account_lanes,
            &refresh_inputs.latest_positions,
            known_open_notional_usd,
            refresh_inputs.cash_balance_assets.as_slice(),
        );
        let mut exceptions = if refresh_inputs.reconciliation_remote_snapshot {
            std::mem::take(&mut refresh_state.exceptions)
        } else {
            Vec::new()
        };
        if refresh_inputs.reconciliation_remote_snapshot {
            exceptions.extend(self.unmapped_managed_open_order_exceptions_for_snapshot(
                account_id,
                snapshot,
                &refresh_inputs.reconciliation_symbols,
            )?);
        }

        openticker_portfolio::apply_account_ledger_refresh_state(
            &mut ledger,
            refresh_state,
            exceptions,
        );
        Ok(())
    }

    pub(crate) fn account_ledger_rooms(
        &self,
        bot_id: &str,
        account_id: &str,
        bot_pct: f64,
    ) -> Result<LedgerRooms, ServiceError> {
        let ledger = self.account_ledger_handle(account_id)?;
        let ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        Ok(openticker_portfolio::ledger_rooms(&ledger, bot_id, bot_pct))
    }

    pub(crate) fn account_risk_snapshot(&self, account_id: &str) -> AccountRiskSnapshot {
        openticker_portfolio::account_risk_snapshot(&self.accounting_lanes_for_account(account_id))
    }

    pub(crate) fn ledger_rejection_event_payload(
        &self,
        lane_id: &str,
        account_id: &str,
        symbol: &str,
        bar: &OhlcvBar,
        intent: TradeIntent,
        ledger_outcome: OrderLedgerOutcome,
    ) -> Result<serde_json::Value, ServiceError> {
        let ledger = self.account_ledger_handle(account_id)?;
        let ledger = ledger.lock().map_err(|_| {
            ServiceError::InvalidConfiguration(format!(
                "capital ledger lock poisoned for account `{account_id}`"
            ))
        })?;
        let lane = self.accounting_lane(lane_id)?;
        let reason_code = ledger_outcome_reason_code(ledger_outcome);
        let intent_label = trade_intent_label(intent);

        let payload = match ledger_outcome {
            OrderLedgerOutcome::BotExhausted => openticker_portfolio::bot_ledger_rejection_payload(
                &ledger,
                account_id,
                &lane.bot_id,
                lane.budget_pct,
                intent_label,
                reason_code,
            ),
            OrderLedgerOutcome::AccountExhausted => {
                openticker_portfolio::account_ledger_rejection_payload(
                    &ledger,
                    account_id,
                    intent_label,
                    reason_code,
                )
            }
            OrderLedgerOutcome::Dust => {
                openticker_portfolio::dust_ledger_rejection_payload(intent_label)
            }
        };

        Ok(serde_json::to_value(
            openticker_portfolio::ledger_rejection_event_payload(
                payload,
                symbol,
                &bar.timestamp.to_rfc3339(),
            ),
        )?)
    }
}

impl Runtime {
    #[must_use]
    pub fn ledger_snapshot(&self) -> LedgerPortfolioSnapshot {
        self.repo().ledger_snapshot()
    }

    pub(crate) fn refresh_account_ledger_from_snapshot(
        &self,
        account_id: &str,
        snapshot: &ConnectorAccountSnapshot,
    ) -> Result<(), ServiceError> {
        self.repo()
            .refresh_account_ledger_from_snapshot(account_id, snapshot)
    }

    pub(crate) fn refresh_account_ledger_from_connector(
        &self,
        account_id: &str,
    ) -> Result<(), ServiceError> {
        match self
            .connector_gateway()
            .fetch_account_snapshot_for_refresh(account_id)
        {
            Ok(snapshot) => self
                .repo()
                .refresh_account_ledger_from_snapshot(account_id, &snapshot),
            Err(ServiceError::InvalidConfiguration(message))
                if message == format!("missing connector status for account `{account_id}`") =>
            {
                Err(ServiceError::InvalidConfiguration(message))
            }
            Err(error) => {
                warn!(account_id, error = %error, "account budget snapshot unavailable");
                self.repo().sync_account_ledger_from_instances(account_id)
            }
        }
    }

    pub(crate) fn refresh_all_account_ledgers_from_connectors(&self) -> Result<(), ServiceError> {
        let mut account_ids = self.catalog.accounts.keys().cloned().collect::<Vec<_>>();
        account_ids.sort();
        for account_id in account_ids {
            self.refresh_account_ledger_from_connector(account_id.as_str())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Runtime;
    use crate::test_support::fixture_bundle;

    #[test]
    fn ledger_snapshot_contains_all_accounts_and_lanes() {
        let bundle = fixture_bundle();
        let runtime = Runtime::from_config(&bundle);
        let snapshot = runtime.ledger_snapshot();
        assert_eq!(snapshot.accounts.len(), bundle.accounts.len());
        assert!(
            !snapshot.bots.is_empty(),
            "fixture bundle should produce bot snapshots"
        );
    }
}
