# openticker-data

Last reviewed: 2026-04-18

Normalized market-data transformation helpers for OpenTicker.

## Purpose

`openticker-data` turns normalized trades into bar updates and provides a lightweight market-session model used by the runtime.

## Current Architecture

The crate revolves around six public concepts:

- `NormalizedTrade`
- `NormalizedQuote`
- `NormalizedOrderEvent`
- `NormalizedBarUpdate`
- `BarBuilder`
- `MarketSession`

Implementation is split by responsibility:

- `src/normalized.rs`: normalized market-data shapes
- `src/bar_builder.rs`: trade-to-bar aggregation and bucket helpers
- `src/market_session.rs`: market-session classification
- `src/error.rs`: crate-local errors
- `src/lib.rs`: stable public re-export surface

## How `BarBuilder` Works

`BarBuilder` (`src/bar_builder.rs`) is the core stateful component.

1. Each incoming `NormalizedTrade` is validated.
2. The trade timestamp is floored into the configured timeframe bucket.
3. If the trade is in the current bucket, the in-progress bar is updated and a `Preview` update is emitted.
4. If the bucket changes, the previous bar is emitted as `Confirmed`, then a new preview bar starts.
5. `flush_confirmed` can emit the final confirmed bar when the caller wants to force close the current bucket.

This is the crate that creates the preview-versus-confirmed distinction used throughout the rest of the system.

## Market Session Model

`market_session_for` (`src/market_session.rs`) currently returns:

- `Continuous` for crypto
- `PreMarket`, `Regular`, or `AfterHours` for equities

The equities window is currently hard-coded in UTC time ranges.

## Current State

- This crate already defines normalized trade, quote, and order-event shapes, but only trade-to-bar aggregation and session classification have behavior today.
- `push_trade` rejects `price <= 0.0` and `quantity < 0.0`, but zero-quantity trades are still accepted.
- Equities session logic is intentionally simple and UTC-based.
- The module split is structural only; public exports and runtime behavior are unchanged.
- The API is small and pure, which makes it a stable dependency for runtime and signal replay.

## Refactor Notes

- If more market-data shapes are added, keep normalization here and venue-specific parsing in `openticker-connectors`.
- Be careful when changing bucket-flooring or flush semantics; many downstream assumptions depend on them.

## Verify

- `cargo test -p openticker-data`
