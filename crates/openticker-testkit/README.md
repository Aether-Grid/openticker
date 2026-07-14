# openticker-testkit

Last reviewed: 2026-06-10

Deterministic test helpers for OpenTicker crates.

## Purpose

`openticker-testkit` exists to hold reusable testing helpers that should not live inside production crates.

## Current Architecture

The crate is intentionally small today and split across contextual source files.

- `src/lib.rs` wires modules and re-exports the public helpers.
- `src/bundle.rs` contains deterministic `ConfigBundle` fixtures.
- `src/fixtures.rs` contains deterministic bar fixtures.
- `src/reconciliation_server.rs` contains the fake reconciliation HTTP server.
- `src/replay.rs` contains replay-oriented helpers.

Its public helpers are:

- `close_only_bar`
- `close_only_symbol_bar`
- `replay_sma_crossover`
- `shared_fixture_bundle`
- `shared_fixture_bundle_for_symbol`
- `spawn_fake_reconciliation_server`

`replay_sma_crossover` constructs an `SmaCrossoverIndicator`, optionally overrides the fast/slow window pair, replays a slice of `OhlcvBar` values through it for a chosen `SignalPhase`, and returns the produced snapshots.

`close_only_bar` creates a deterministic single-bar OHLCV fixture from an RFC3339 timestamp and a close price.

`close_only_symbol_bar` wraps `close_only_bar` and returns a deterministic `(symbol, bar)` pair.

`shared_fixture_bundle` and `shared_fixture_bundle_for_symbol` build a deterministic single-instance paper `ConfigBundle` for runtime-style tests.

`spawn_fake_reconciliation_server` spawns a deterministic fake HTTP server for Alpaca-style reconciliation snapshots (`GET /v2/orders`, `GET /v2/positions`, `GET /v2/account`) and returns the observed request lines on join.

## Current State

- This crate is still at an early stage.
- It is not yet a general replay harness for all indicators.
- Its public surface is six targeted helpers rather than a broad fake-runtime or fixture platform.

## Refactor Notes

- If more reusable test scaffolding appears, this crate is the correct home for deterministic replay helpers, fake inputs, and assertion-friendly wrappers.
- Keep production logic out of this crate even if tests happen to need it.

## Verify

- `cargo test -p openticker-testkit`
