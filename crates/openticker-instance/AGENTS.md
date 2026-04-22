# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns config-driven indicator and strategy assembly plus per-bar
indicator evaluation semantics.

`openticker-instance` should remain the pure runtime-wiring layer between
`openticker-config`, `openticker-signals`, and `openticker-strategy`.

## Package And Commands

- Cargo package: `openticker-instance`
- Main file: `src/lib.rs`
- Verify: `cargo test -p openticker-instance`

## Current Working Shape

- `RuntimeIndicatorEngine` is a concrete enum over supported indicators.
- `build_runtime_indicator_engine(...)` uses string dispatch from
  `IndicatorInstanceConfig.indicator_type`.
- `build_runtime_indicators(...)` applies role, signal-policy, metadata, and
  weight defaults.
- `build_runtime_strategy(...)` currently supports
  `single_indicator_signal` and `consensus`.
- `evaluate_indicator_signals(...)` enforces preview vs confirmed behavior by
  cloning indicator engines for preview evaluations.
- `required_warmup_bars(...)` derives warmup defaults from indicator manifest
  metadata.

## Invariants

- Preserve deterministic preview vs confirmed signal semantics.
- Keep signal-policy coercion for `SignalMode::ConfirmedOnly` intact.
- Keep unknown indicator and strategy errors explicit and contextual.
- Keep this crate free of connector I/O, storage access, and runtime mutation.

## Common Change Recipes

### Add a new runtime indicator type

1. Add indicator implementation and manifest metadata in
   `crates/openticker-signals`.
2. Add enum variant and dispatch wiring in `RuntimeIndicatorEngine` and
   `build_runtime_indicator_engine(...)`.
3. Ensure `type_id(...)` and evaluation dispatch include the new variant.
4. Add focused tests for parameter parsing and preview/confirmed behavior.

### Add a new runtime strategy type

1. Implement strategy in `crates/openticker-strategy`.
2. Add selection path in `build_runtime_strategy(...)`.
3. Update runtime callers and config docs/examples as needed.

## Watchouts

- Indicator type support is still manually mirrored in this crate and can drift
  from signals manifest coverage.
- Preview evaluation clones full indicator engines and may become expensive as
  indicator state grows.

## Common Follow-Ups

- Update `crates/openticker-runtime/src/runtime_wiring.rs` when public helper
  contracts here change.
- Update `crates/openticker-config` if new indicator params or strategy keys
  need validation updates.
