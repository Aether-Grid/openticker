# AGENTS.md

Last reviewed: 2026-04-22

## Overview

This crate owns the object-safe indicator contract, shared indicator helpers, built-in default/example indicators, and their built-in manifest metadata. It is pure computation plus observability logging.

## Package And Commands

- Cargo package: `openticker-signals`
- Main files: `src/lib.rs`, `src/engine.rs`, `src/common/mod.rs`, `src/manifest.rs`, `src/observability.rs`, `src/registry.rs`, `src/indicators/mod.rs`
- Verify: `cargo test -p openticker-signals`

## Current Working Shape

- `IndicatorEngine` is the common contract.
- `src/common/` holds reusable math and time-series helpers (`rolling.rs`, `crossings.rs`, `params.rs`); `src/engine.rs` holds the `IndicatorEngine` contract re-exported from `src/lib.rs`.
- `manifest.rs` is the built-in capability and classification table derived from the built-in descriptor registry.
- Built-in indicator modules now live under `src/indicators/` and are re-exported from `src/lib.rs`.
- `registry.rs` exposes the built-in descriptor list used by the cross-crate indicator registry.
- Each indicator module owns its params, snapshot, engine state, and error type.

## Invariants

- Keep indicator logic pure and deterministic.
- Update `manifest.rs` whenever you add, rename, remove, or materially reclassify an indicator.
- Preserve preview-versus-confirmed and stability-class intent.
- Shared math helpers belong in `src/common/` only when reuse is real.

## Common Change Recipes

### Add a new indicator

1. Add a new module under `src/indicators/` with params, snapshot, engine state, descriptor, and tests.
2. Export the module from `src/indicators/mod.rs` (and keep it reachable through `src/lib.rs`).
3. If the indicator is deployable, confirm the build-specific registry surface can see it and update `openticker-config` validation only if new rules are needed downstream.
4. Add replay or golden coverage when behavior is important enough to snapshot.

### Change an indicator's role or safety classification

1. Update the manifest entry.
2. Re-check config validation expectations in `openticker-config`.
3. Re-check runtime behavior if the role affects strategy composition.

## Watchouts

- The built-in manifest is descriptor-backed now, but private extension indicators are aggregated outside this crate by `openticker-registry`.
- Actionable-signal logging goes through `log_indicator_evaluation`; keep that distinction useful for operators.

## Common Follow-Ups

- Update `crates/openticker-config` when manifest rules change.
- Update `crates/openticker-registry` only when build-specific registry behavior changes.
