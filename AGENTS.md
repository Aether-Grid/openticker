# AGENTS.md

## Overview

This is a multi-crate Rust workspace for configurable spot-trading runtime orchestration. The codebase is architecture-driven and crate-separated, but several implementation-heavy crates are still internally monolithic. When working here, optimize for preserving crate boundaries while making the smallest correct change.

## Workspace Commands

- Format: `make fmt`
- Format check: `make fmt-check`
- Check: `make check`
- Build: `make build`
- Test: `make test`
- Lint: `make lint`
- CI-style pass: `make ci`

Cargo equivalents:

- `cargo check --workspace`
- `cargo build --workspace`
- `cargo test --workspace`
- `cargo lint`

Useful operator commands:

- validate config:
  `cargo run -p openticker-cli -- validate-config --config-dir config`
- start service:
  `cargo run -p openticker-cli -- service run --config-dir config`
- run dashboard:
  `cargo run -p openticker-cli -- dashboard`

## Workspace Shape

Implementation crates live in `crates/`.

Important crate roles:

- `openticker-core`: shared domain types
- `openticker-config`: config schema and validation
- `openticker-signals`: indicator implementations and manifest metadata
- `openticker-strategy`: signal-to-intent mapping
- `openticker-risk`: pure risk policy
- `openticker-data`: normalized market-data transformation
- `openticker-dataplane`: always-on stream scheduler and in-memory bar retention
- `openticker-execution`: venue-neutral execution contracts
- `openticker-ledger`: ownership accounting, reservations, and portfolio snapshots
- `openticker-lane`: extracted per-lane runtime state and lane-local DTOs
- `openticker-storage`: runtime journaling and persistence
- `openticker-connectors`: venue adapters and connector registry
- `openticker-runtime`: runtime composition root
- `openticker-http`: Axum control plane
- `openticker-cli`: operator CLI and TUI
- `openticker-testkit`: reusable test helpers

## Naming Watchout

Use `openticker-runtime` as the actual crate and package name.

## Where To Read First

For workspace-level context, read:

1. `README.md`
2. the nearest crate-local `README.md`
3. the nearest crate-local `AGENTS.md`

This repository intentionally omits the internal planning and audit docs that used to live under `docs/`.

## Important Cross-Crate Realities

- `openticker-runtime` is the composition root and contains most orchestration.
- `openticker-dataplane` owns stream registry, polling cadence, and per-stream buffers.
- `openticker-http` owns the HTTP API and wires the dataplane task into the running service.
- `openticker-signals` owns manifest metadata, but runtime indicator construction is still manual in `openticker-runtime`.
- `openticker-config` duplicates some connector capability knowledge that also exists conceptually in `openticker-connectors`.
- `openticker-storage` is the audit and restart substrate for the runtime.

## Common Change Recipes

### Add a new indicator

1. Implement the indicator in `openticker-signals`.
2. Add or update the manifest entry there.
3. Update config validation in `openticker-config` if the indicator introduces new rules or parameters.
4. Wire runtime instantiation in `openticker-runtime`.
5. Add or update tests in the signals crate and runtime integration tests if behavior affects orchestration.

### Add a new connector

1. Implement the adapter in `openticker-connectors`.
2. Update connector construction and capability metadata there.
3. Update config validation in `openticker-config`.
4. Add runtime-facing tests if polling, execution, reconciliation, or stream normalization changes.

### Add a new HTTP endpoint

1. Add the handler and route in `openticker-http`.
2. Update the generated OpenAPI route list if the endpoint is public.
3. Update `openticker-cli` if operator command mode or the dashboard should consume it.
4. Add tests.

### Change persisted runtime records

1. Update `openticker-storage` types, trait methods, both backends, and SQLite schema together.
2. Update runtime call sites in `openticker-runtime`.
3. Update HTTP and CLI consumers if returned shapes change.

### Change runtime lifecycle, reconciliation, or processing

1. Start in `openticker-runtime`.
2. Trace the impact through `openticker-http`, `openticker-cli`, and `openticker-storage`.
3. Prefer integration tests over only unit tests.

## Invariants To Preserve

- Keep crate boundaries meaningful. Do not move connector-specific logic into core crates.
- Keep risk evaluation pure in `openticker-risk`.
- Keep signal logic pure in `openticker-signals`.
- Keep venue-specific payload types from leaking out of `openticker-connectors`.
- Preserve explicit live-mode warnings in operator-facing surfaces.
- Preserve startup reconciliation and journal-backed recovery behavior in `openticker-runtime`.

## Testing Strategy

- For workspace-wide confidence, use `make test` or `cargo test --workspace`.
- For focused work, prefer package-targeted tests first.
- When changing runtime orchestration, polling, reconciliation, or connector behavior, run the affected runtime integration tests, not just the unit tests of the crate you edited.

## Final Rule

The nearest crate-local `AGENTS.md` should guide the actual implementation details once you know which crate you are changing. This root file is for workspace-wide navigation and cross-crate coordination.
