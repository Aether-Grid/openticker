# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate is the Axum control plane over `openticker-runtime`. It is the contract surface consumed by the CLI dashboard and command mode.

## Package And Commands

- Cargo package: `openticker-http`
- Module root: `src/lib.rs`
- Verify: `cargo test -p openticker-http`

## Current Working Shape

- Route constants and OpenAPI route descriptors live in `src/constants.rs`.
- Router wiring lives in `src/router.rs`.
- Handler implementations live in `src/handlers.rs`.
- `load_http_state` and `serve` live in `src/runtime.rs`.
- Runtime-owned background polling is provided by
  `openticker-runtime::RuntimePollingSupervisor`.
- `HttpState` owns the runtime lock, loaded config bundle, and background-polling metrics.
- Integration-style HTTP tests currently live in `src/lib.rs`.

## Invariants

- Keep handler outputs normalized and JSON-first unless the endpoint is explicitly text or HTML.
- Keep the generated OpenAPI route list in sync with the actual router.
- Handlers should stay thin and delegate runtime behavior to `openticker-runtime`.

## Common Change Recipes

### Add a new endpoint

1. Add the path constant if it is part of the public surface.
2. Add or update the route descriptor in `HTTP_SURFACE_ROUTES` if it should appear in OpenAPI.
3. Register the route in `build_router` (`src/router.rs`).
4. Add the handler function (`src/handlers.rs`).
5. Update CLI and dashboard consumers if they should use the new endpoint.
6. Add or update tests.

### Change status or metrics output

1. Update the handler or metrics text rendering.
2. Confirm the runtime already exposes the needed information.
3. Update downstream CLI or dashboard assumptions if response shape changes.

## Watchouts

- Polling behavior ownership now lives in `openticker-runtime`; this crate should
  consume runtime-owned polling state rather than hosting scheduler logic.
- Route-shape changes are likely to break `openticker-cli` quickly.
- Keep `src/constants.rs` route descriptors and `src/router.rs` route registrations aligned.

## Common Follow-Ups

- Update `crates/openticker-cli` when endpoint shapes or paths change.
- Update `crates/openticker-runtime` if new control-plane behavior requires new runtime methods.
