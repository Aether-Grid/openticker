# openticker-testkit

Last reviewed: 2026-04-22

Deterministic test helpers for OpenTicker crates.

## Purpose

`openticker-testkit` exists to hold reusable testing helpers that should not live inside production crates.

## Current Architecture

The crate is intentionally small today and split across contextual source files.

- `src/lib.rs` wires modules and re-exports the public helpers.
- `src/replay.rs` contains replay-oriented helpers.
- `src/fixtures.rs` contains deterministic bar fixtures.

Its public helpers are:

- `replay_sma_crossover`
- `close_only_bar`

`replay_sma_crossover` constructs an `SmaCrossoverIndicator`, optionally overrides the fast/slow window pair, replays a slice of `OhlcvBar` values through it for a chosen `SignalPhase`, and returns the produced snapshots.

`close_only_bar` creates a deterministic single-bar OHLCV fixture from an RFC3339 timestamp and a close price.

## Current State

- This crate is still at an early stage.
- It is not yet a general replay harness for all indicators.
- Its public surface is two targeted helpers rather than a broad fake-runtime or fixture platform.

## Refactor Notes

- If more reusable test scaffolding appears, this crate is the correct home for deterministic replay helpers, fake inputs, and assertion-friendly wrappers.
- Keep production logic out of this crate even if tests happen to need it.

## Verify

- `cargo test -p openticker-testkit`
