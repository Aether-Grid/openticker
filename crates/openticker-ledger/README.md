# openticker-ledger

Last reviewed: 2026-04-18

Capital accounting and portfolio bookkeeping for OpenTicker.

## Purpose

`openticker-ledger` owns the pure accounting layer that sits between runtime
orchestration and operator-facing portfolio views. It is the home for owner-path
state, capital reservations, blocking exceptions, and portfolio rollups.

## First Slice

The first implementation slice focuses on:

- explicit owner paths for `(account, bot, symbol)` lanes
- reservation state separate from attributed open exposure
- account-blocking ledger exceptions for ambiguous or unmatched ownership
- account, bot, and lane portfolio snapshots for runtime and HTTP consumption

Cost basis, fees, valuation marks, and full P/L accounting remain follow-up work
for later phases of the ledger plan.

## Verify

- `cargo test -p openticker-ledger`
