# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns config-driven indicator and strategy assembly plus per-bar
indicator evaluation semantics over boxed indicator engines.

`openticker-instance` should remain the pure runtime-wiring layer between
`openticker-config`, `openticker-registry`, and `openticker-strategy`.

## Package And Commands

- Cargo package: `openticker-instance`
- Main file: `src/lib.rs`
- Verify: `cargo test -p openticker-instance`

## Current Working Shape

- `ConfiguredIndicatorRuntime.engine` is a boxed `IndicatorEngine` trait object.
- `build_runtime_indicator_engine(...)` delegates string dispatch from
  `IndicatorInstanceConfig.indicator_type` to `openticker-registry`.
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

1. Add indicator implementation and descriptor metadata in
   `crates/openticker-signals` for built-ins or `crates/openticker-indicators`
   for private extensions.
2. Ensure the owning crate exports the descriptor and the build-specific
   registry can see it.
3. Add focused tests for parameter parsing and preview/confirmed behavior.

### Add a new runtime strategy type

1. Implement strategy in `crates/openticker-strategy`.
2. Add selection path in `build_runtime_strategy(...)`.
3. Update runtime callers and config docs/examples as needed.

## Watchouts

- Indicator type support now comes from the build-specific registry. Keep this
  crate free of concrete indicator imports.
- Preview evaluation clones full indicator engines and may become expensive as
  indicator state grows.

## Common Follow-Ups

- Update `crates/openticker-runtime/src/runtime_wiring.rs` when public helper
  contracts here change.
- Update `crates/openticker-config` if new indicator params or strategy keys
  need validation updates.
