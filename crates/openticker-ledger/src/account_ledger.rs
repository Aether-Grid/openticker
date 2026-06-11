use std::collections::HashMap;

use crate::portfolio::{AccountPortfolioSnapshot, BotPortfolioSnapshot, LanePortfolioSnapshot};
use crate::types::{LedgerError, LedgerException, LedgerOwnerPath, ReservationError};
use crate::util::{LEDGER_VALUE_TOLERANCE, sanitize_value};

#[derive(Debug, Clone, Default)]
pub struct AccountLedger {
    declared_total_usd: f64,
    live_balance_usd: Option<f64>,
    lane_open_notional_usd: HashMap<LedgerOwnerPath, f64>,
    lane_reserved_notional_usd: HashMap<LedgerOwnerPath, f64>,
    unattributed_open_notional_usd: f64,
    exceptions: Vec<LedgerException>,
}

impl AccountLedger {
    #[must_use]
    pub fn new(declared_total_usd: f64) -> Self {
        Self {
            declared_total_usd: sanitize_value(declared_total_usd),
            live_balance_usd: None,
            lane_open_notional_usd: HashMap::new(),
            lane_reserved_notional_usd: HashMap::new(),
            unattributed_open_notional_usd: 0.0,
            exceptions: Vec::new(),
        }
    }

    #[must_use]
    pub fn effective_cap_usd(&self) -> f64 {
        match self.live_balance_usd {
            Some(live_balance_usd) => self.declared_total_usd.min(live_balance_usd.max(0.0)),
            None => self.declared_total_usd,
        }
    }

    pub fn set_live_balance_usd(&mut self, live_balance_usd: Option<f64>) {
        self.live_balance_usd = live_balance_usd.map(sanitize_value);
    }

    pub fn set_unattributed_open_notional_usd(&mut self, notional_usd: f64) {
        self.unattributed_open_notional_usd = sanitize_value(notional_usd);
    }

    pub fn replace_exceptions(&mut self, exceptions: Vec<LedgerException>) {
        self.exceptions = exceptions;
    }

    pub fn replace_lane_open_notional(
        &mut self,
        entries: impl IntoIterator<Item = (LedgerOwnerPath, f64)>,
    ) {
        self.lane_open_notional_usd = entries
            .into_iter()
            .filter_map(|(owner, notional_usd)| {
                let notional_usd = sanitize_value(notional_usd);
                if notional_usd > LEDGER_VALUE_TOLERANCE {
                    Some((owner, notional_usd))
                } else {
                    None
                }
            })
            .collect();
    }

    /// Reserves additional open notional for an owner path.
    ///
    /// # Errors
    ///
    /// Returns `ReservationError::BotCapacityExceeded` when the owning bot has
    /// no remaining allocation and `ReservationError::AccountCapacityExceeded`
    /// when account-level tradeable room is exhausted.
    pub fn try_reserve_open(
        &mut self,
        owner: &LedgerOwnerPath,
        notional_usd: f64,
        bot_pct: f64,
    ) -> Result<(), ReservationError> {
        let notional_usd = sanitize_value(notional_usd);
        if notional_usd <= LEDGER_VALUE_TOLERANCE {
            return Ok(());
        }

        let bot_tradeable_open_room_usd =
            self.bot_tradeable_open_room_usd(owner.bot_id.as_str(), bot_pct);
        if notional_usd > bot_tradeable_open_room_usd + LEDGER_VALUE_TOLERANCE {
            return if self.bot_available_open_room_usd(owner.bot_id.as_str(), bot_pct)
                <= LEDGER_VALUE_TOLERANCE
            {
                Err(ReservationError::BotCapacityExceeded)
            } else {
                Err(ReservationError::AccountCapacityExceeded)
            };
        }

        let entry = self
            .lane_reserved_notional_usd
            .entry(owner.clone())
            .or_insert(0.0);
        *entry += notional_usd;
        Ok(())
    }

    /// Releases previously reserved open notional for an owner path.
    ///
    /// The release amount must be a finite, strictly positive USD value. The
    /// effect is clamped at a zero floor: releasing more than is currently
    /// reserved removes the owner's reservation entirely rather than driving
    /// the tracked total negative (reconciliation can legitimately request an
    /// over-release due to rounding or out-of-order events), but such a valid
    /// positive request still returns `Ok`.
    ///
    /// # Errors
    ///
    /// Returns `LedgerError::InvalidReleaseAmount` when `notional_usd` is
    /// non-finite (`NaN`/`Inf`) or non-positive (`<= 0`). Such inputs are not
    /// valid release amounts; rejecting them avoids silently trapping reserved
    /// notional (the prior `sanitize_value` behavior turned them into no-ops).
    pub fn release_reservation(
        &mut self,
        owner: &LedgerOwnerPath,
        notional_usd: f64,
    ) -> Result<(), LedgerError> {
        if !notional_usd.is_finite() || notional_usd <= 0.0 {
            return Err(LedgerError::InvalidReleaseAmount);
        }
        adjust_owner_notional(&mut self.lane_reserved_notional_usd, owner, -notional_usd);
        Ok(())
    }

    pub fn reconcile_open_fill(
        &mut self,
        owner: &LedgerOwnerPath,
        filled_notional_usd: f64,
        reserved_notional_usd: f64,
    ) {
        let filled_notional_usd = sanitize_value(filled_notional_usd);
        let reserved_notional_usd = sanitize_value(reserved_notional_usd);

        if reserved_notional_usd > LEDGER_VALUE_TOLERANCE {
            // `reserved_notional_usd` was sanitized above and is strictly
            // positive and finite here, so the release can never be invalid.
            let release_result = self.release_reservation(owner, reserved_notional_usd);
            debug_assert!(
                release_result.is_ok(),
                "sanitized reserved notional must be a valid release amount"
            );
        }

        if filled_notional_usd > LEDGER_VALUE_TOLERANCE {
            adjust_owner_notional(&mut self.lane_open_notional_usd, owner, filled_notional_usd);
        }
    }

    /// Releases attributed open position notional for an owner path.
    ///
    /// The release amount must be a finite, strictly positive USD value. The
    /// effect is clamped at a zero floor: releasing more than is currently
    /// attributed removes the owner's open notional entirely rather than
    /// driving the tracked total negative, but such a valid positive request
    /// still returns `Ok`.
    ///
    /// # Errors
    ///
    /// Returns `LedgerError::InvalidReleaseAmount` when `notional_usd` is
    /// non-finite (`NaN`/`Inf`) or non-positive (`<= 0`). Such inputs are not
    /// valid release amounts; rejecting them avoids silently trapping open
    /// notional (the prior `sanitize_value` behavior turned them into no-ops).
    pub fn release_position(
        &mut self,
        owner: &LedgerOwnerPath,
        notional_usd: f64,
    ) -> Result<(), LedgerError> {
        if !notional_usd.is_finite() || notional_usd <= 0.0 {
            return Err(LedgerError::InvalidReleaseAmount);
        }
        adjust_owner_notional(&mut self.lane_open_notional_usd, owner, -notional_usd);
        Ok(())
    }

    #[must_use]
    pub fn bot_allocated_usd(&self, pct: f64) -> f64 {
        (self.effective_cap_usd() * pct / 100.0).max(0.0)
    }

    #[must_use]
    pub fn bot_attributed_open_notional_usd(&self, bot_id: &str) -> f64 {
        owner_group_total(&self.lane_open_notional_usd, bot_id)
    }

    #[must_use]
    pub fn bot_reserved_open_notional_usd(&self, bot_id: &str) -> f64 {
        owner_group_total(&self.lane_reserved_notional_usd, bot_id)
    }

    #[must_use]
    pub fn bot_committed_notional_usd(&self, bot_id: &str) -> f64 {
        self.bot_attributed_open_notional_usd(bot_id) + self.bot_reserved_open_notional_usd(bot_id)
    }

    #[must_use]
    pub fn total_attributed_open_notional_usd(&self) -> f64 {
        self.lane_open_notional_usd.values().copied().sum()
    }

    #[must_use]
    pub fn total_reserved_open_notional_usd(&self) -> f64 {
        self.lane_reserved_notional_usd.values().copied().sum()
    }

    #[must_use]
    pub fn total_committed_notional_usd(&self) -> f64 {
        self.total_attributed_open_notional_usd()
            + self.unattributed_open_notional_usd
            + self.total_reserved_open_notional_usd()
    }

    #[must_use]
    pub fn account_available_open_room_usd(&self) -> f64 {
        (self.effective_cap_usd() - self.total_committed_notional_usd()).max(0.0)
    }

    #[must_use]
    pub fn account_tradeable_open_room_usd(&self) -> f64 {
        if self.has_blocking_exceptions() {
            0.0
        } else {
            self.account_available_open_room_usd()
        }
    }

    #[must_use]
    pub fn account_blocked_open_room_usd(&self) -> f64 {
        self.account_available_open_room_usd() - self.account_tradeable_open_room_usd()
    }

    #[must_use]
    pub fn bot_available_open_room_usd(&self, bot_id: &str, pct: f64) -> f64 {
        (self.bot_allocated_usd(pct) - self.bot_committed_notional_usd(bot_id)).max(0.0)
    }

    #[must_use]
    pub fn bot_tradeable_open_room_usd(&self, bot_id: &str, pct: f64) -> f64 {
        self.bot_available_open_room_usd(bot_id, pct)
            .min(self.account_tradeable_open_room_usd())
    }

    #[must_use]
    pub fn bot_blocked_open_room_usd(&self, bot_id: &str, pct: f64) -> f64 {
        self.bot_available_open_room_usd(bot_id, pct)
            - self.bot_tradeable_open_room_usd(bot_id, pct)
    }

    #[must_use]
    pub fn has_blocking_exceptions(&self) -> bool {
        self.exceptions
            .iter()
            .any(|exception| exception.blocks_new_opens)
    }

    #[must_use]
    pub fn exceptions(&self) -> &[LedgerException] {
        &self.exceptions
    }

    #[must_use]
    pub fn account_snapshot(&self, account_id: &str) -> AccountPortfolioSnapshot {
        let mut exceptions = self.exceptions.clone();
        exceptions.sort_by(|left, right| {
            left.symbol
                .cmp(&right.symbol)
                .then_with(|| left.kind.as_str().cmp(right.kind.as_str()))
                .then_with(|| left.detail.cmp(&right.detail))
        });

        AccountPortfolioSnapshot {
            id: account_id.to_owned(),
            declared_total_usd: self.declared_total_usd,
            live_balance_usd: self.live_balance_usd,
            effective_cap_usd: self.effective_cap_usd(),
            attributed_open_notional_usd: self.total_attributed_open_notional_usd(),
            unattributed_open_notional_usd: self.unattributed_open_notional_usd,
            reserved_open_notional_usd: self.total_reserved_open_notional_usd(),
            total_committed_notional_usd: self.total_committed_notional_usd(),
            blocked_open_room_usd: self.account_blocked_open_room_usd(),
            tradeable_open_room_usd: self.account_tradeable_open_room_usd(),
            exceptions,
        }
    }

    #[must_use]
    pub fn bot_snapshot(&self, account_id: &str, bot_id: &str, pct: f64) -> BotPortfolioSnapshot {
        BotPortfolioSnapshot {
            id: bot_id.to_owned(),
            account_id: account_id.to_owned(),
            pct,
            allocated_usd: self.bot_allocated_usd(pct),
            attributed_open_notional_usd: self.bot_attributed_open_notional_usd(bot_id),
            reserved_open_notional_usd: self.bot_reserved_open_notional_usd(bot_id),
            total_committed_notional_usd: self.bot_committed_notional_usd(bot_id),
            blocked_open_room_usd: self.bot_blocked_open_room_usd(bot_id, pct),
            tradeable_open_room_usd: self.bot_tradeable_open_room_usd(bot_id, pct),
        }
    }

    #[must_use]
    pub fn lane_snapshots(&self) -> Vec<LanePortfolioSnapshot> {
        let mut owners = self
            .lane_open_notional_usd
            .keys()
            .chain(self.lane_reserved_notional_usd.keys())
            .cloned()
            .collect::<Vec<_>>();
        owners.sort_by(|left, right| {
            left.account_id
                .cmp(&right.account_id)
                .then_with(|| left.bot_id.cmp(&right.bot_id))
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        owners.dedup();

        owners
            .into_iter()
            .filter_map(|owner| {
                let attributed_open_notional_usd = self
                    .lane_open_notional_usd
                    .get(&owner)
                    .copied()
                    .unwrap_or(0.0);
                let reserved_open_notional_usd = self
                    .lane_reserved_notional_usd
                    .get(&owner)
                    .copied()
                    .unwrap_or(0.0);
                let total_committed_notional_usd =
                    attributed_open_notional_usd + reserved_open_notional_usd;
                if total_committed_notional_usd <= LEDGER_VALUE_TOLERANCE {
                    None
                } else {
                    Some(LanePortfolioSnapshot {
                        owner,
                        attributed_open_notional_usd,
                        reserved_open_notional_usd,
                        total_committed_notional_usd,
                    })
                }
            })
            .collect()
    }
}

fn owner_group_total(entries: &HashMap<LedgerOwnerPath, f64>, bot_id: &str) -> f64 {
    entries
        .iter()
        .filter_map(|(owner, notional_usd)| {
            if owner.bot_id == bot_id {
                Some(*notional_usd)
            } else {
                None
            }
        })
        .sum()
}

fn adjust_owner_notional(
    entries: &mut HashMap<LedgerOwnerPath, f64>,
    owner: &LedgerOwnerPath,
    delta_usd: f64,
) {
    let next_value = entries.get(owner).copied().unwrap_or(0.0) + delta_usd;
    if next_value > LEDGER_VALUE_TOLERANCE {
        entries.insert(owner.clone(), next_value);
    } else {
        entries.remove(owner);
    }
}

#[cfg(test)]
mod tests {
    use super::AccountLedger;
    use crate::{
        LedgerError, LedgerException, LedgerExceptionKind, LedgerOwnerPath, ReservationError,
    };

    #[test]
    fn effective_cap_uses_declared_total_until_live_balance_is_lower() {
        let mut ledger = AccountLedger::new(1_000.0);

        assert!((ledger.effective_cap_usd() - 1_000.0).abs() < 1e-6);

        ledger.set_live_balance_usd(Some(1_200.0));
        assert!((ledger.effective_cap_usd() - 1_000.0).abs() < 1e-6);

        ledger.set_live_balance_usd(Some(800.0));
        assert!((ledger.effective_cap_usd() - 800.0).abs() < 1e-6);
    }

    #[test]
    fn partial_open_fill_releases_unused_reservation() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);

        assert!(ledger.try_reserve_open(&owner, 400.0, 50.0).is_ok());
        assert!((ledger.total_reserved_open_notional_usd() - 400.0).abs() < 1e-6);
        assert!(ledger.total_attributed_open_notional_usd().abs() < 1e-6);

        ledger.reconcile_open_fill(&owner, 250.0, 400.0);
        assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
        assert!((ledger.total_attributed_open_notional_usd() - 250.0).abs() < 1e-6);
        assert!((ledger.total_committed_notional_usd() - 250.0).abs() < 1e-6);
    }

    #[test]
    fn fill_larger_than_reserved_does_not_double_count_committed_notional() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);

        assert!(ledger.try_reserve_open(&owner, 200.0, 100.0).is_ok());
        ledger.reconcile_open_fill(&owner, 300.0, 200.0);

        assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
        assert!((ledger.total_attributed_open_notional_usd() - 300.0).abs() < 1e-6);
        assert!((ledger.total_committed_notional_usd() - 300.0).abs() < 1e-6);
    }

    #[test]
    fn blocking_exception_zeroes_tradeable_room_without_corrupting_available_room() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);
        ledger.replace_lane_open_notional([(owner, 250.0)]);
        ledger.replace_exceptions(vec![LedgerException {
            kind: LedgerExceptionKind::AmbiguousSymbolOwner,
            owner: None,
            symbol: Some("MSFT".to_owned()),
            detail: "matching_bots=bot-a,bot-b".to_owned(),
            blocks_new_opens: true,
        }]);

        let snapshot = ledger.account_snapshot("acct");
        assert!((ledger.account_available_open_room_usd() - 750.0).abs() < 1e-6);
        assert!((snapshot.attributed_open_notional_usd - 250.0).abs() < 1e-6);
        assert!(snapshot.unattributed_open_notional_usd.abs() < 1e-6);
        assert!(snapshot.tradeable_open_room_usd.abs() < 1e-6);
        assert!((snapshot.blocked_open_room_usd - 750.0).abs() < 1e-6);
    }

    #[test]
    fn lane_snapshots_are_sorted_and_skip_zero_committed_rows() {
        let owner_a = LedgerOwnerPath::new("acct", "bot-b", "MSFT");
        let owner_b = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let owner_c = LedgerOwnerPath::new("acct", "bot-c", "NVDA");
        let mut ledger = AccountLedger::new(1_000.0);
        ledger.replace_lane_open_notional([(owner_a.clone(), 100.0), (owner_b.clone(), 50.0)]);
        assert!(ledger.try_reserve_open(&owner_c, 25.0, 100.0).is_ok());
        assert!(ledger.release_reservation(&owner_c, 25.0).is_ok());

        let snapshots = ledger.lane_snapshots();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].owner, owner_b);
        assert_eq!(snapshots[1].owner, owner_a);
    }

    #[test]
    fn reservation_release_is_idempotent_under_small_dust_values() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);

        assert!(ledger.try_reserve_open(&owner, 100.0, 100.0).is_ok());
        assert!(ledger.release_reservation(&owner, 100.0 - 5e-10).is_ok());
        assert!(ledger.release_reservation(&owner, 1.0).is_ok());

        assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);
        assert!(ledger.lane_snapshots().is_empty());
    }

    #[test]
    fn bot_and_account_caps_both_apply_to_reservations() {
        let bot_a = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let bot_b = LedgerOwnerPath::new("acct", "bot-b", "MSFT");
        let mut ledger = AccountLedger::new(1_000.0);

        assert!(ledger.try_reserve_open(&bot_a, 400.0, 40.0).is_ok());
        assert_eq!(
            ledger.try_reserve_open(&bot_a, 10.0, 40.0),
            Err(ReservationError::BotCapacityExceeded)
        );

        assert!(ledger.try_reserve_open(&bot_b, 500.0, 60.0).is_ok());
        assert_eq!(
            ledger.try_reserve_open(&LedgerOwnerPath::new("acct", "bot-c", "NVDA"), 200.0, 30.0,),
            Err(ReservationError::AccountCapacityExceeded)
        );
    }

    #[test]
    fn release_reservation_rejects_non_positive_and_non_finite_amounts() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);
        assert!(ledger.try_reserve_open(&owner, 400.0, 100.0).is_ok());

        for invalid in [-100.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                ledger.release_reservation(&owner, invalid),
                Err(LedgerError::InvalidReleaseAmount),
                "expected rejection for release amount {invalid}"
            );
        }

        // Rejected releases must leave the reservation untouched (no silent
        // no-op that traps notional, and no accidental mutation).
        assert!((ledger.total_reserved_open_notional_usd() - 400.0).abs() < 1e-6);
    }

    #[test]
    fn release_position_rejects_non_positive_and_non_finite_amounts() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);
        ledger.replace_lane_open_notional([(owner.clone(), 300.0)]);

        for invalid in [-50.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                ledger.release_position(&owner, invalid),
                Err(LedgerError::InvalidReleaseAmount),
                "expected rejection for release amount {invalid}"
            );
        }

        assert!((ledger.total_attributed_open_notional_usd() - 300.0).abs() < 1e-6);
    }

    #[test]
    fn valid_positive_release_succeeds_and_decrements() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);
        assert!(ledger.try_reserve_open(&owner, 400.0, 100.0).is_ok());

        assert!(ledger.release_reservation(&owner, 150.0).is_ok());
        assert!((ledger.total_reserved_open_notional_usd() - 250.0).abs() < 1e-6);

        ledger.replace_lane_open_notional([(owner.clone(), 300.0)]);
        assert!(ledger.release_position(&owner, 120.0).is_ok());
        assert!((ledger.total_attributed_open_notional_usd() - 180.0).abs() < 1e-6);
    }

    #[test]
    fn over_release_clamps_at_zero_floor_and_still_returns_ok() {
        let owner = LedgerOwnerPath::new("acct", "bot-a", "AAPL");
        let mut ledger = AccountLedger::new(1_000.0);
        assert!(ledger.try_reserve_open(&owner, 100.0, 100.0).is_ok());

        // Releasing more than is reserved is a valid positive request: it must
        // succeed and clamp the tracked total at zero (never go negative).
        assert!(ledger.release_reservation(&owner, 500.0).is_ok());
        assert!(ledger.total_reserved_open_notional_usd().abs() < 1e-6);

        ledger.replace_lane_open_notional([(owner.clone(), 80.0)]);
        assert!(ledger.release_position(&owner, 500.0).is_ok());
        assert!(ledger.total_attributed_open_notional_usd().abs() < 1e-6);
    }
}
