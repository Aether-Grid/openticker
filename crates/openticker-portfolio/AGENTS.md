# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns pure portfolio-accounting helpers used by runtime accounting and
reconciliation read models.

`openticker-portfolio` should stay side-effect free and focus on:

- ledger snapshot composition and ordering
- lane/account risk rollups
- connector-position ownership resolution
- connector-snapshot to ledger-refresh mapping

## Package And Commands

- Cargo package: `openticker-portfolio`
- Main file: `src/lib.rs` (re-exports only; implementation lives in focused modules under `src/`)
- Verify: `cargo test -p openticker-portfolio`

## Current Working Shape

- `PortfolioLaneView`, `LatestLanePosition`, and `AccountLedgerRefreshState`
  are the main helper DTOs.
- `ledger_snapshot(...)` returns sorted account/bot/lane output from ledger
  snapshot parts.
- `connector_position_owner(...)` resolves owner lanes from configured lanes and
  latest journaled positions.
- `connector_position_exceptions(...)` emits ownership exceptions for unmatched
  and ambiguous connector positions.
- `live_balance_from_snapshot(...)` currently maps balances via account-kind
  specific logic (`alpaca`, `binance`).

## Invariants

- Keep this crate pure: no connector registry calls, storage writes, or runtime
  orchestration.
- Preserve deterministic sort order in snapshot output.
- Preserve deterministic ownership resolution for `(account, symbol)` matching.
- Keep exception generation explicit rather than silently ignoring mismatches.

## Common Change Recipes

### Add or change account refresh mapping

1. Update `account_ledger_refresh_state(...)` and supporting helpers.
2. Keep lane-notional, live-balance, and exception outputs aligned.
3. Add focused tests for connector snapshots and resulting exceptions.
4. Verify runtime accounting call sites continue to pass correct lane and
   position inputs.

### Change connector-position ownership rules

1. Update `connector_position_owner(...)` and symbol-match helpers.
2. Keep ambiguity handling explicit in `connector_position_exceptions(...)`.
3. Add tests for no-match, unique-match, and ambiguous-match outcomes.

## Watchouts

- Owner resolution currently depends on string-based position reasons such as
  `_reconciliation_sync` and `close_requested`.
- Symbol matching currently relies on a small quote-suffix list.
- Account balance interpretation currently branches on account kind strings.

## Common Follow-Ups

- Update `crates/openticker-runtime` when snapshot, exception, or ownership
  helper contracts change.
- Update `crates/openticker-ledger` if exception kinds or snapshot shapes change.
