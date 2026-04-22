# openticker-http

Last reviewed: 2026-04-19

HTTP control-plane API for OpenTicker.

## Purpose

`openticker-http` exposes `openticker-runtime` over a local-first Axum API. It serves health and readiness checks, metrics, config inspection, connector and service status, bot lifecycle actions, and a lightweight embedded dashboard.

## Current Architecture

The crate is now split by concern:

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Crate root, public re-exports, and integration-style HTTP tests |
| `src/constants.rs` | Public route constants, OpenAPI route descriptor list, OpenAPI document generation, shared constants |
| `src/state.rs` | `HttpState` and HTTP response or projection structs |
| `src/router.rs` | `build_router(...)` and Axum route wiring |
| `src/handlers.rs` | Endpoint handlers plus handler-local helper logic |
| `src/runtime.rs` | `load_http_state(...)`, `serve(...)`, and runtime-owned polling supervisor wiring |
| `static/dist/index.html` | Embedded dashboard frontend served at `/` and `/dashboard` |

## Route Groups

The current route set includes:

- `/healthz`, `/readyz`, `/metrics`, `/openapi.json`
- `/dashboard` and `/`
- `/v1/service/status`, `/v1/ledger`, `/v1/ledger/accounts`, `/v1/ledger/bots`, and `/v1/ledger/lanes`
- `/v1/config/reload`, `/v1/config/effective`
- `/v1/connectors/matrix`, `/v1/connectors/status`
- `/v1/data/streams` and `/v1/data/streams/{account}/{symbol}/{timeframe}/bars`
- `/v1/events`, `/v1/signals`, `/v1/intents`, `/v1/risk-decisions`, `/v1/orders`, `/v1/fills`, `/v1/positions`, `/v1/reconciliations`
- `/v1/bots` plus per-bot lifecycle, simulation, reconciliation-report, lanes, manual-signal, and manual control endpoints
- `/v1/risk/kill-switch` and `/v1/risk/clear-kill-switch`

## How It Works

1. `load_http_state` loads config from disk and builds a storage-backed `Runtime`.
2. `serve` binds the configured address, starts `RuntimePollingSupervisor`, and serves Axum routes.
3. `build_router` wires handlers and HTTP tracing middleware.
4. Handlers use read or write locks around runtime state and return normalized JSON (except text or HTML endpoints).
5. `/metrics` renders Prometheus-style text from runtime status and dataplane polling metrics.
6. `/openapi.json` is generated from the route descriptor list in `constants.rs`.

## Background Polling

The periodic polling loop now lives in `openticker-runtime` via
`RuntimePollingSupervisor`.

This crate starts and shuts down the supervisor from `runtime.rs`, while the
runtime-owned loop:

- runs `DataPlane::run_forever(...)`
- fetches bars through connector callbacks
- dispatches appended bars back into runtime for completion and persistence
- records polling latency, runtime write-lock wait, and completion counters in
  dataplane metrics

## Current State

- The crate is modularized, but `handlers.rs` is still large and contains multiple endpoint domains.
- The dashboard source lives in `static/src/pages/index.astro`, and Rust embeds `static/dist/index.html`.
- Handler outputs are intentionally JSON-first because the CLI and dashboard both consume them.

## Refactor Notes

- If route surface area keeps growing, split `handlers.rs` into endpoint-domain modules.
- Keep polling ownership in `openticker-runtime`; this crate should stay focused
  on control-plane wiring and handler behavior.
- Keep route additions aligned with the generated OpenAPI route list and CLI/dashboard consumers.

## Verify

- `cd crates/openticker-http/static && npm ci && npm run build`
- `cargo test -p openticker-http`
