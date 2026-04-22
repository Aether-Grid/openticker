# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This directory contains the operator-facing CLI and Ratatui dashboard. The crate is intentionally thin and should mostly translate operator actions into config loads or HTTP API calls.

## Package And Commands

- Cargo package: `openticker-cli`
- Main files: `src/main.rs`, `src/cli.rs`, `src/api.rs`, `src/tracing_setup.rs`, `src/commands/*`, `src/dashboard.rs`
- Verify: `cargo test -p openticker-cli`
- Manual checks:
  - `cargo run -p openticker-cli -- validate-config --config-dir config`
  - `cargo run -p openticker-cli -- dashboard`

## Current Working Shape

- `src/main.rs` is a thin entrypoint that initializes tracing, parses CLI args, and dispatches commands.
- `src/cli.rs` owns Clap enums and option structs.
- `src/commands/*` owns command handlers split by concern (`config`, `service`, `risk`, `instance`).
- `src/api.rs` owns generic HTTP request and JSON-print helpers used by command mode.
- `src/dashboard.rs` owns snapshot fetching, terminal rendering, and keyboard-bound instance operations.
- The crate depends on `openticker-http` for service startup and API contracts.

## Invariants

- Keep business logic out of this crate when possible.
- Preserve explicit live-mode warnings in command output and dashboard views.
- Prefer driving actions through the HTTP API rather than reaching into runtime internals.

## Common Change Recipes

### Add a new CLI command

1. Add the enum variant in `src/cli.rs`.
2. Update the relevant handler in `src/commands/*` (and `src/commands/mod.rs` if adding a new command group).
3. Reuse `fetch_and_print`, `post_and_print`, or other helpers in `src/api.rs` when possible.
4. If the command needs new backend behavior, add or change the HTTP endpoint in `openticker-http` first.

### Add a new dashboard action or pane

1. Extend `DashboardSnapshot` and `fetch_snapshot`.
2. Add the rendering logic in `src/dashboard.rs`.
3. Wire the key binding through the input loop and `BotOperation` or a new explicit action.
4. Confirm the related endpoint exists and returns stable JSON.

## Watchouts

- Response-shape changes in `openticker-http` can silently break both command output and dashboard deserialization.
- `service run` starts the HTTP server in-process; do not duplicate runtime boot logic locally.
- The default API URL is local-first. Keep that bias unless the user asks otherwise.

## Common Follow-Ups

- Update `crates/openticker-http` if you need a new inspection or lifecycle endpoint.
- Update `crates/openticker-runtime` if a new operator action requires new runtime behavior.
