# AGENTS.md

Last reviewed: 2026-04-22

## Overview

This crate owns indicator behavior, manifests, and shared indicator helpers. It is pure computation plus observability logging.

## Package And Commands

- Cargo package: `openticker-signals`
- Main files: `src/lib.rs`, `src/common.rs`, `src/manifest.rs`, `src/observability.rs`, `src/signals/mod.rs`
- Verify: `cargo test -p openticker-signals`

## Current Working Shape

- `IndicatorEngine` is the common contract.
- `common.rs` holds reusable math and time-series helpers.
- `manifest.rs` is the capability and classification table for all built-in indicators.
- Built-in indicator modules now live under `src/signals/` and are re-exported from `src/lib.rs`.
- Each indicator module owns its params, snapshot, engine state, and error type.

## Invariants

- Keep indicator logic pure and deterministic.
- Update `manifest.rs` whenever you add, rename, remove, or materially reclassify an indicator.
- Preserve preview-versus-confirmed and stability-class intent.
- Shared math helpers belong in `common.rs` only when reuse is real.

## Common Change Recipes

### Add a new indicator

1. Add a new module under `src/signals/` with params, snapshot, engine state, and tests.
2. Export the module from `src/signals/mod.rs` (and keep it reachable through `src/lib.rs`).
3. Add manifest metadata in `src/manifest.rs`.
4. If the indicator is deployable, update `openticker-runtime` factory wiring and `openticker-config` validation downstream.
5. Add replay or golden coverage when behavior is important enough to snapshot.

### Change an indicator's role or safety classification

1. Update the manifest entry.
2. Re-check config validation expectations in `openticker-config`.
3. Re-check runtime behavior if the role affects strategy composition.

## Watchouts

- The manifest and runtime factory are separate sources today. Supporting a new type in this crate is not enough by itself.
- Actionable-signal logging goes through `log_indicator_evaluation`; keep that distinction useful for operators.

## Common Follow-Ups

- Update `crates/openticker-config` when manifest rules change.
- Update `crates/openticker-runtime` when new indicator modules must be instantiated at runtime.
