# openticker-signals

Last reviewed: 2026-04-22

Indicator contracts, manifests, and built-in signal implementations for OpenTicker.

## Purpose

`openticker-signals` is the workspace home for the object-safe indicator contract and the built-in OSS-safe default/example indicators. It ships the built-in descriptors and manifests for those indicators and emits observability logs for evaluation.

## Current Architecture

- `src/lib.rs`: module wiring and public re-exports for the indicator contract, built-in manifests, and descriptor types
- `src/engine.rs`: the object-safe indicator contract (`IndicatorEngine`, `SignalSnapshot`) and evaluation/build/descriptor types
- `src/common/`: shared math and series-building helpers (rolling stats in `rolling.rs`, crossover/crossunder in `crossings.rs`, TOML param parsing in `params.rs`)
- `src/manifest.rs`: built-in manifest helpers derived from the built-in descriptor registry
- `src/registry.rs`: built-in indicator descriptor helpers
- `src/indicators/mod.rs`: built-in indicator module declarations
- `src/indicators/*.rs`: one module per built-in indicator implementation

## Built-In Indicators

- `sma_crossover`: a fast-vs-slow simple moving average crossover signal intended as a primary directional signal
- `rsi_threshold`: a standard RSI threshold indicator intended for filter or context roles

## Current State

- Shared math helpers live in `src/common/`.
- Runtime construction now happens through `openticker-registry`.
- The built-in manifest is descriptor-backed, so built-in metadata and built-in construction stay in one place.
- Coverage currently focuses on deterministic unit tests for the built-in indicators plus manifest contract tests.

## Verify

- `cargo test -p openticker-signals`
