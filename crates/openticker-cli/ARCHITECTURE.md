# ARCHITECTURE

Last reviewed: 2026-04-18

## Role

`openticker-cli` is the operator-facing binary crate.

It owns two surfaces:

- command-oriented CLI operations
- the Ratatui dashboard

Architecturally, it is intentionally thin. It should mostly translate operator intent into either:

- local config loading
- in-process HTTP service startup
- HTTP API calls against the running control plane

## Entry Surface

Because this is a binary crate, the process entrypoint is `src/main.rs`, with command logic split into dedicated modules.

Important command-side functions:

- `main()` in `src/main.rs`
- `run()` in `src/main.rs`
- `dispatch_command(...)` in `src/commands/mod.rs`
- `validate_config(...)` and `handle_config_command(...)` in `src/commands/config.rs`
- `handle_service_command(...)` and `run_service(...)` in `src/commands/service.rs`
- `handle_risk_command(...)` in `src/commands/risk.rs`
- `handle_instance_command(...)` and `run_auto_tick(...)` in `src/commands/instance.rs`
- `api_request_json(...)`, `fetch_and_print(...)`, `post_and_print(...)`, and `extract_live_mode_banner(...)` in `src/api.rs`

Important dashboard-side functions and types in `src/dashboard.rs`:

- `dashboard::run(...)`
- `DashboardApp`
- `fetch_snapshot(...)`
- `BotOperation`

## Internal Layout

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Thin binary entrypoint and command dispatch bootstrapping |
| `src/cli.rs` | Clap command tree and shared CLI option types |
| `src/commands/mod.rs` | Top-level command dispatch |
| `src/commands/config.rs` | `validate-config` and `config` command handling |
| `src/commands/service.rs` | `service` command handling and in-process service startup |
| `src/commands/risk.rs` | `risk` command handling |
| `src/commands/instance.rs` | `instance` command handling and `auto-tick` loop |
| `src/api.rs` | Generic HTTP request helpers, JSON printing, live-mode warning extraction |
| `src/tracing_setup.rs` | Tracing subscriber and file logging setup |
| `src/dashboard.rs` | Ratatui state, snapshot fetching, key handling, rendering, tests |

Logical split:

1. command enums and shared options (`src/cli.rs`)
2. command dispatch and grouped handlers (`src/commands/*`)
3. generic HTTP request helpers and JSON printing (`src/api.rs`)
4. tracing and log bootstrap (`src/tracing_setup.rs`)
5. dashboard state and rendering loop (`src/dashboard.rs`)
6. dashboard snapshot fetch and actions (`src/dashboard.rs`)

## Direct Dependency Wiring

Workspace dependencies:

| Crate | Used For |
| --- | --- |
| `openticker-config` | local config validation and effective-config printing |
| `openticker-http` | in-process service startup and API contract surface |

## Inbound Wiring

The only true inbound dependency is the operator.

Users enter through:

- CLI commands
- the dashboard TUI

## Outbound Wiring

This crate fans out in three ways:

- direct config path: `openticker_config::load_from_dir(...)`
- in-process service startup path: `openticker_http::load_http_state(...)` then `serve(...)`
- normal operational path: HTTP calls to the local control-plane endpoints

The dashboard is also HTTP-driven. It does not reach into runtime internals directly.

## Operator Flow

### Command mode

1. parse Clap command tree
2. route command by group through `src/commands/mod.rs`
3. decide whether the command is local-config, service-start, or HTTP-driven
4. execute the relevant path
5. optionally surface explicit live-mode warnings
6. print JSON or text output

### Dashboard mode

1. start the terminal UI loop
2. fetch a multi-endpoint snapshot from the HTTP API
3. render panes for service, instances, connectors, and stream state
4. map key presses into `BotOperation` values
5. send lifecycle or operator actions back through HTTP

## Current Implementation Realities

- Command-side code is now split by concern (`cli`, `commands`, `api`, `tracing_setup`) while dashboard logic remains in one large file.
- There is no shared typed API client; many command paths work with generic `serde_json::Value` output.
- Endpoint paths and response-shape assumptions are duplicated in command handlers and dashboard code.
- Live-mode warnings are extracted heuristically from returned JSON rather than from a dedicated typed contract.
- The dashboard fetches several endpoints separately instead of consuming one aggregate snapshot endpoint.
- `service run` starts the HTTP server in-process; this crate does not boot runtime directly.
- `cancel-open-orders` is still request-only today, while `close-positions` now goes through the runtime's normal execution path.

## Practical Wiring Notes

- If operator functionality changes, `openticker-http` usually changes first.
- Business logic should stay out of this crate where possible.
- Default control-plane usage is local-first and should remain biased toward local operation.

## Diagram

```mermaid
flowchart TD
  User[Operator] --> CLI[CLI command or dashboard]
  CLI --> Config[openticker-config]
  CLI --> HTTP[openticker-http API]
  CLI --> Serve[openticker-http serve()]
  HTTP --> Runtime[Runtime through HTTP]
```
