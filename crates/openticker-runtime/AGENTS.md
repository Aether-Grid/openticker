# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate is the runtime composition root. It owns lifecycle, indicator and
strategy wiring, signal-to-order processing, reconciliation, ledger-backed
budgeting, and journal-backed read models.

## Package And Commands

- Cargo package: `openticker-runtime`
- Entry file: `src/lib.rs` (module wiring plus crate-level imports and
  re-exports)
- Core runtime modules:
  - `src/construction.rs`
  - `src/lifecycle.rs`
  - `src/market_data/`
  - `src/polling_supervisor.rs`
  - `src/processing/`
  - `src/reconciliation/`
  - `src/manual_ops.rs`
  - `src/portfolio_adapter.rs`
  - `src/queries/`
  - `src/persistence.rs`
  - `src/connector_gateway.rs`
- Shared support modules:
  - `src/model/`
  - `src/shared/`
  - `src/runtime_wiring.rs`
  - `src/errors.rs`
- Integration tests:
  - `tests/stock_paper_end_to_end.rs`
  - `tests/stock_reconciliation_restart.rs`
  - `tests/crypto_kline_ingestion.rs`
- Verify: `cargo test -p openticker-runtime`

## Current Working Shape

- `Runtime` owns lanes, account config, account ledgers, connector registry,
  kill switch, observability, and runtime journal.
- Runtime-owned background polling lifecycle is hosted by
  `RuntimePollingSupervisor` in `src/polling_supervisor.rs` and coordinates
  dataplane scheduling with runtime dispatch.
- `LaneRuntime` owns per-lane state: indicator runtimes, strategy engine,
  bar-builder and warmup state, risk limits, position state, and connector
  execution constraints.
- `src/model/` holds internal runtime state structs and public API/read-model
  types re-exported from `src/lib.rs`.
- `openticker-lane` now owns extracted lane state structs, lane bootstrap and
  recovered-state helpers, and lane-local internal DTOs plus the extracted lane
  cycle and polling workflow algorithms; warmup backfill logic is now on the
  same boundary. Execution/journaling sub-workflows now follow the same adapter
  boundary, and lane-local manual close sequencing now follows the same
  pattern. runtime re-exports compatible types while the larger engine split is
  still in progress.
- `src/runtime_wiring.rs` now just holds runtime-focused tests around lane-owned
  indicator and strategy assembly.
- `src/shared/` holds shared helper logic for labels, budgets, inventory sync,
  sizing, event logging, connector/status mapping, and symbol helpers.
- `src/repo/` now also owns the reusable provider-event stage logger used by
  `src/connector_gateway.rs`, plus repo-backed accounting/reconciliation helper
  assembly and journal bootstrap reads; the read-side accounting surface no longer lives in
  `src/portfolio_adapter.rs`.
- `src/processing/constraints.rs` now owns the lane-local connector symbol
  constraint initialization workflow, so `src/connector_gateway.rs` is a
  thinner read-only runtime adapter.
- `src/processing/cycle.rs` now owns the runtime-side lane-cycle adapter and
  cycle-trace persistence plumbing, so `src/processing/pipeline.rs` is mostly
  the public process-bar and manual-signal entrypoint layer. Some larger
  scenario tests now live in sibling `*_tests.rs` files so those modules stay
  focused on runtime behavior.
- `src/processing/executor_engine.rs` now owns the runtime-side
  `LaneExecutionEngine` adapter, while `src/processing/executor.rs` keeps the
  higher-level runtime-facing execution wrapper methods.
- Connector-facing methods are now split by domain as well: market-data
  gateway wrappers live under `src/market_data/`, execution submission under
  `src/processing/`, and snapshot outcome logging under `src/reconciliation/`.
- `src/market_data/targets.rs` now owns shared poll-target resolution,
  stream-key derivation, and connector-backed bar fetch helpers.
- `src/market_data/dataplane.rs` now owns dataplane stream registration and
  stream-history lookup, while `src/market_data/dispatch.rs` owns runtime bar
  dispatch and stream-level polling fan-in.
- `src/market_data/recovery_engine.rs` now owns the runtime-side lane-polling
  adapter plus recovery-state and recovery-event plumbing, so
  `recovery.rs` focuses on the public recovery entrypoints and tests.
- `src/market_data/warmup_engine.rs` now owns the runtime-side warmup adapter
  and warmup-failure plumbing, so `warmup.rs` focuses on the public warmup
  entrypoints and uses a sibling `warmup_tests.rs` for larger scenarios.
- `src/reconciliation/` is split into orchestration, assessment, and apply
  sections with scenario-grouped tests.
- The main processing path is `process_bar` in
  `src/processing/pipeline.rs`, with planning in `src/processing/planner.rs`.

## Invariants

- Preserve startup reconciliation and readiness safety.
- Preserve explicit live-mode and reconciliation-blocked behavior.
- Keep journal writes aligned with runtime state transitions.
- Keep ledger synchronization aligned with account and lane state updates.
- Be careful with package naming in commands and docs: use
  `openticker-runtime`.

## Common Change Recipes

### Add a new indicator type

1. Add the indicator implementation in `openticker-signals` for built-ins or `openticker-indicators` for private extensions.
2. Register its descriptor in the owning crate.
3. Update config validation if new parameters or rules are needed.
4. Add runtime tests if the indicator changes processing behavior materially.

### Add a new strategy type

1. Implement it in `openticker-strategy`.
2. Add runtime selection in `build_runtime_strategy`.
3. Update config examples and tests if the strategy is deployable.

### Change signal-to-order behavior

1. Inspect `process_bar` in `src/processing/pipeline.rs` and
   `evaluate_process_bar` in `src/processing/planner.rs` first.
2. Confirm changes still flow through strategy, quantity resolution, risk,
   execution, and journal recording.
3. Update integration tests, not just unit tests.

### Change reconciliation or lifecycle rules

1. Review startup boot flow in `src/construction.rs` plus instance transition
   methods in `src/lifecycle.rs` and `src/reconciliation/`.
2. Update reconciliation report behavior if operator-visible semantics change.
3. Expect HTTP and CLI consumers to need updates.

## Watchouts

- `src/market_data/` is split structurally, but trade ingestion, polling,
  stream dispatch, and warmup flow remain tightly coupled even though polling
  recovery control flow now delegates to `openticker-lane`.
- `src/connector_gateway.rs` is now a much smaller shared adapter; the larger
  remaining runtime surfaces are mostly domain modules. `src/construction.rs`
  is now mostly boot orchestration, and `src/portfolio_adapter.rs` is much
  smaller and mostly limited to the mutable ledger-rejection event path, while
  runtime-facing ledger snapshot/refresh entrypoints now sit under
  `src/repo/accounting.rs`.
- `src/processing/` is split structurally, but the pipeline is still tightly
  coupled across planner, executor, journal, and state updates.
- Runtime indicator support now flows through `openticker-instance` and the
  build-specific registry rather than local runtime wiring.
- `evaluate_process_bar` still uses placeholder stale-data, spread, and
  slippage inputs. Do not assume those signals are already fully wired.

## Common Follow-Ups

- Update `crates/openticker-http` when runtime status or lifecycle contracts
  change.
- Update `crates/openticker-cli` when operator-visible outputs or supported
  actions change.
- Update `crates/openticker-storage` if persisted record shapes or recovery
  assumptions change.
- Update `crates/openticker-ledger` if accounting and ownership contracts
  change.
