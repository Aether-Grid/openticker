# AGENTS.md

Last reviewed: 2026-06-10

## Overview

This crate is for reusable deterministic test helpers. It is intentionally small today.

## Package And Commands

- Cargo package: `openticker-testkit`
- Entry file: `src/lib.rs` (module wiring and re-exports)
- Contextual files: `src/bundle.rs`, `src/fixtures.rs`, `src/reconciliation_server.rs`, `src/replay.rs`
- Verify: `cargo test -p openticker-testkit`

## Current Working Shape

- `replay_sma_crossover` (in `src/replay.rs`) is the current replay helper.
- `close_only_bar` and `close_only_symbol_bar` (in `src/fixtures.rs`) are the bar-fixture helpers.
- `shared_fixture_bundle` and `shared_fixture_bundle_for_symbol` (in `src/bundle.rs`) are the config-bundle helpers.
- `spawn_fake_reconciliation_server` (in `src/reconciliation_server.rs`) is the fake reconciliation server helper.
- All helpers are explicit and deterministic by design.

## Invariants

- Keep helpers deterministic and side-effect free.
- Do not move production runtime logic here just because tests use it.
- Reuse real workspace contracts instead of copying test-only equivalents.

## Common Change Recipes

### Add a new replay helper

1. Keep the helper explicit about which indicator or subsystem it targets.
2. Favor small deterministic helpers over generic but opaque abstractions.
3. Reuse production contracts from `openticker-core` and `openticker-signals` rather than copying them.

### Add a new fixture helper

1. Keep the helper tiny and assertion-friendly.
2. Make timestamps and values deterministic at the call site.
3. Avoid embedding runtime bootstrapping or storage behavior here.

## Watchouts

- This crate is not yet a general test harness; avoid overdesigning it.

## Common Follow-Ups

- Update consuming tests in other crates when helper signatures change.
