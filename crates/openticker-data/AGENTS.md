# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns normalized trade-to-bar transformation logic. It is small but behaviorally important because preview and confirmed semantics start here.

## Package And Commands

- Cargo package: `openticker-data`
- Public entry: `src/lib.rs`
- Behavior modules: `src/bar_builder.rs`, `src/market_session.rs`
- Data shape module: `src/normalized.rs`
- Verify: `cargo test -p openticker-data`

## Current Working Shape

- `BarBuilder` is the central stateful component.
- `market_session_for` is the only market-session helper.
- The API is intentionally pure and free of connector or runtime side effects.
- `src/lib.rs` is intentionally thin and should remain a re-export surface.

## Invariants

- Preserve preview-versus-confirmed behavior in `BarBuilder`.
- Keep bucket flooring deterministic.
- Leave venue-specific payload parsing in `openticker-connectors`.

## Common Change Recipes

### Change bar aggregation behavior

1. Update `BarBuilder` carefully.
2. Keep the transition semantics between preview and confirmed explicit.
3. Update or add focused tests for same-bucket updates, bucket rollover, and flush behavior.

### Add new normalized market-data shapes

1. Keep the types venue-neutral.
2. Add transformation helpers here.
3. Wire raw connector payload parsing in `openticker-connectors`, not here.

## Watchouts

- Changes in flooring or flush behavior can ripple into runtime processing, signal replay, and tests.

## Common Follow-Ups

- Update `crates/openticker-runtime` and any replay-oriented tests when the sequencing contract changes.
