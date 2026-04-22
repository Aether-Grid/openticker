# openticker-core

Last reviewed: 2026-04-18

Canonical shared domain types for the OpenTicker workspace.

## Purpose

`openticker-core` is the narrowest shared domain crate in the workspace. It carries the enums and small value types that other crates agree on when talking about markets, execution mode, bars, signal phases, indicator roles, and trade intents.

## Current Architecture

The crate is intentionally small and now uses a thin module split, with `src/lib.rs` as the public re-export surface.

Internal files:

- `src/error.rs`
- `src/identifiers.rs`
- `src/market.rs`
- `src/signals.rs`
- `src/timeframe.rs`
- `src/trade.rs`

The main public pieces are:

- market and execution enums: `MarketType`, `ExecutionMode`
- timeframe model with string parsing and serde handling: `Timeframe`
- identifier wrappers with basic validation: `InstanceId`, `AccountId`, `BotLaneKey`
- bar representation: `OhlcvBar`
- signal and intent enums: `SignalPhase`, `IndicatorSignal`, `TradeIntent`
- indicator metadata enums: `IndicatorRole`, `IndicatorStabilityClass`, `IndicatorSignalPolicy`
- lightweight errors: `CoreError`

## How It Works

- `src/lib.rs` re-exports all public contracts so downstream crates keep importing from `openticker_core` directly.
- `Timeframe` is serialized as strings such as `1m`, `15m`, or `1d`.
- `InstanceId`, `AccountId`, and `BotLaneKey` currently only enforce non-empty trimmed values through `parse()`.
- Signal-phase and trade-intent enums are used across the config, runtime, strategy, and HTTP layers.

## Current State

- This crate is intentionally smaller than the broader architecture docs describe. It does not yet contain order, fill, position, quote, or book models.
- Only a small typed-identifier set exists today (`InstanceId`, `AccountId`, `BotLaneKey`); there are no separate typed connector or strategy IDs yet.
- `Timeframe` is one of the most reused contracts in the workspace, so even small changes here have wide downstream impact.

## Refactor Notes

- Prefer small, stable additions over broad redesigns here.
- If a type is only used by one outer crate, it probably does not belong in `openticker-core` yet.
- If a future type needs storage- or connector-specific details, keep it out of this crate and normalize at the boundary instead.

## Verify

- `cargo test -p openticker-core`
