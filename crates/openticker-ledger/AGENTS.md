# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns pure ownership accounting and portfolio bookkeeping.

`openticker-ledger` should model capital state transitions, reservations, and
portfolio snapshots without pulling in runtime orchestration, connector
transport details, or HTTP concerns.

## Package And Commands

- Cargo package: `openticker-ledger`
- Main files:
  - `src/account_ledger.rs`
  - `src/inventory.rs`
  - `src/portfolio.rs`
  - `src/types.rs`
  - `src/util.rs`
- Verify: `cargo test -p openticker-ledger`

## Current Working Shape

- `AccountLedger` is the core state container for account-budget ownership and
  per-owner-path allocation.
- `inventory.rs` owns fill-side, lot, fee, realized and unrealized PnL domain
  types used by ledger-level bookkeeping.
- `portfolio.rs` owns account, bot, and lane snapshot DTOs exported to runtime
  and control-plane surfaces.
- `types.rs` owns owner-path, ownership policy, reservation, and ledger
  exception types.
- `lib.rs` re-exports the crate public API and keeps module boundaries explicit.

## Invariants

- Keep this crate pure and deterministic: no connector I/O and no runtime
  orchestration logic.
- Preserve owner-path attribution semantics for `(account, bot, symbol)`.
- Keep reservation state separate from attributed open exposure.
- Prefer explicit exceptions over silently dropping ambiguous ownership states.

## Common Change Recipes

### Add a new snapshot field

1. Update snapshot types in `src/portfolio.rs`.
2. Update `AccountLedger` snapshot builders in `src/account_ledger.rs`.
3. Update runtime and HTTP consumers that serialize or render those snapshots.
4. Add or update tests in this crate first.

### Add a new ownership or reservation rule

1. Extend domain types in `src/types.rs` where needed.
2. Implement state transition logic in `src/account_ledger.rs`.
3. Add deterministic tests for normal and exceptional paths.
4. Ensure runtime callers handle any new exception kind.

## Watchouts

- Avoid introducing runtime-specific assumptions into ledger APIs.
- Avoid embedding formatting or operator-facing message construction here.
- Keep arithmetic behavior stable and explicit; prefer helper functions over
  repeated ad hoc calculations.

## Common Follow-Ups

- Update `crates/openticker-runtime` when ledger API shapes change.
- Update `crates/openticker-http` and `crates/openticker-cli` when snapshot
  output shapes or naming change.
