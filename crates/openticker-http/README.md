# openticker-http

Last reviewed: 2026-07-14

HTTP control-plane API for OpenTicker.

## Purpose

`openticker-http` exposes `openticker-runtime` over a local-first Axum API. It serves health and readiness checks, metrics, config inspection, connector and service status, bot lifecycle actions, and a lightweight embedded dashboard.

## Current Architecture

The crate is now split by concern:

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Small crate root and public re-exports |
| `src/constants.rs` | Public route paths, embedded dashboard assets, limits, and timeouts |
| `src/openapi.rs` | OpenAPI route descriptor list and generated document |
| `src/state.rs` | `HttpState` and HTTP response or projection structs |
| `src/router.rs` | `build_router(...)` and Axum route wiring |
| `src/handlers/` | Endpoint-domain handlers and shared HTTP error/query helpers |
| `src/config_ops.rs` | Config reload validation, application, and status tracking |
| `src/config_watcher.rs` | Debounced on-disk config change monitoring |
| `src/config_write_handlers.rs` | Validated config mutation endpoints |
| `src/runtime.rs` | `load_http_state(...)`, `serve(...)`, and runtime-owned polling supervisor wiring |
| `src/tests/` | Domain-grouped in-process router tests |
| `static/dist/index.html` | Embedded dashboard frontend served at `/` and `/dashboard` |

## Route Groups

The current route set includes:

- `/healthz`, `/readyz`, `/metrics`, `/openapi.json`
- `/dashboard` and `/`
- `/v1/service/status`, `/v1/ledger`, `/v1/ledger/accounts`, `/v1/ledger/bots`, and `/v1/ledger/lanes`
- `/v1/config/reload`, `/v1/config/reload/status`, `/v1/config/effective`, and validated config write routes
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
6. `/openapi.json` is generated from the route descriptor list in `openapi.rs`.
7. Optional bearer-token authentication, request-body limits, and bounded journal queries are applied at the HTTP boundary.

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

- Handlers are grouped by endpoint domain under `src/handlers/`; shared transport concerns remain in `handlers/mod.rs`.
- Managed config supports status/history reporting, validated writes, and filesystem-watched reloads.
- The dashboard source lives in `static/src/pages/index.astro`, and Rust embeds `static/dist/index.html`.
- Handler outputs are intentionally JSON-first because the CLI and dashboard both consume them.

## Refactor Notes

- Keep new handlers in the matching endpoint-domain module instead of growing `handlers/mod.rs`.
- Keep polling ownership in `openticker-runtime`; this crate should stay focused
  on control-plane wiring and handler behavior.
- Keep route additions aligned with the generated OpenAPI route list and CLI/dashboard consumers.

## Verify

- `cd crates/openticker-http/static && npm ci && npm run build`
- `cargo test -p openticker-http`
