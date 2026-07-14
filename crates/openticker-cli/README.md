# openticker-cli

Last reviewed: 2026-04-18

Operator-facing command-line and terminal UI tooling for OpenTicker.

## Purpose

`openticker-cli` is the local control surface for the workspace. It does three things:

- validates config directly from disk
- starts the HTTP-backed service in-process for local operation
- acts as a thin API client for status, risk, budget, and instance lifecycle commands

It also contains the Ratatui dashboard for an always-on operator view.

## Current Architecture

- `src/main.rs`
  Thin binary entrypoint: tracing init, CLI parse, and command dispatch.
- `src/cli.rs`
  Clap command tree and shared CLI option types.
- `src/commands/`
  Command handlers split by domain (`config`, `service`, `risk`, `instance`) plus top-level dispatch.
- `src/api.rs`
  Shared HTTP request/print helpers, journaling-only warning output, and live-mode banner extraction.
- `src/tracing_setup.rs`
  Tracing and file logging bootstrap.
- `src/dashboard/`
  Implements the interactive Ratatui dashboard. It polls the HTTP API for a composite snapshot and binds keyboard actions to instance and kill-switch endpoints.

There is intentionally very little business logic here. The crate mostly translates operator intent into HTTP requests or local config loading.

## Command Surface

The current command tree is grouped into:

- `dashboard`
- `validate-config`
- `config print`
- `config reload`
- `service run`
- `service status`
- `service budget`
- `service connectors`
- `service connectors-matrix`
- `service events`, `signals`, `intents`, `risk-decisions`, `orders`, `fills`, `positions`, `reconciliations`
- `risk kill-switch on|off`
- `risk status`
- `instance list|get|start|stop|pause|resume|reconcile|reconcile-report|tick|auto-tick|cancel-open-orders|close-positions`

## How It Works

1. `main` initializes tracing once and writes logs to stderr plus a daily JSONL file.
2. Clap parses the command tree defined in `src/cli.rs`.
3. `dispatch_command` in `src/commands/mod.rs` routes to domain handlers.
4. Pure config commands call `openticker_config::load_from_dir` directly.
5. Service and instance commands call the HTTP API exposed by `openticker-http` and pretty-print JSON responses.
6. `service run` is the special case: `src/commands/service.rs` loads config, builds `HttpState` through `openticker_http::load_http_state`, and then calls `openticker_http::serve`.

## Dashboard Internals

The dashboard is an API-driven UI, not a direct runtime UI.

- `DashboardSnapshot` is assembled by `fetch_snapshot` from multiple endpoints.
- The instance table is the primary selection surface.
- Keyboard actions map to instance lifecycle or risk-control endpoints.
- The current keymap includes start, stop, pause, resume, tick, reconcile, cancel open orders, close positions, and kill switch actions.

This architecture keeps the dashboard aligned with the same contracts used by command mode and any future web UI.

## Current State

- The crate is intentionally thin and delegates almost everything to `openticker-http`.
- API URL defaults to `http://127.0.0.1:8080`.
- Live-mode warnings are inferred from JSON payloads via `extract_live_mode_banner` so operator output stays explicit even for generic JSON commands.
- The dashboard fetches several views separately rather than consuming one server-side aggregate endpoint.

## Refactor Notes

- Command-side code is now split by concern; add new commands by extending `src/cli.rs` and the matching `src/commands/*` module.
- Dashboard polling, state, input, and rendering are split into focused modules under `src/dashboard/`.
- If response shapes change in `openticker-http`, this crate is usually the first downstream consumer that will need updates.

## Verify

- `cargo test -p openticker-cli`
- `cargo run -p openticker-cli -- validate-config --config-dir config`
- `cargo run -p openticker-cli -- dashboard`
