# openticker-signals

Last reviewed: 2026-04-22

Indicator contracts, manifests, and built-in signal implementations for OpenTicker.

## Purpose

`openticker-signals` is the workspace home for indicator behavior. It defines the shared update contract, ships the built-in OSS-safe indicators, classifies them through manifests, and emits observability logs for evaluation.

## Current Architecture

- `src/lib.rs`: public re-exports for the indicator contract and manifests
- `src/common.rs`: shared math and series-building helpers such as EMA, SMA, Wilder RMA, ATR, RSI, Supertrend, crossover, and crossunder
- `src/manifest.rs`: the static registry for supported built-in indicators
- `src/signals/mod.rs`: indicator module declarations
- `src/signals/*.rs`: one module per indicator implementation

## Built-In Indicators

- `sma_crossover`: a fast-vs-slow simple moving average crossover signal intended as a primary directional signal
- `rsi_threshold`: a standard RSI threshold indicator intended for filter or context roles

## Current State

- Shared math helpers live in `common.rs`.
- Runtime construction still happens outside this crate.
- The manifest is authoritative for classification, but runtime instantiation is still manually mirrored in `openticker-instance`.
- Coverage currently focuses on deterministic unit tests for the built-in indicators plus manifest contract tests.

## Verify

- `cargo test -p openticker-signals`
