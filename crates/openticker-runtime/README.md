# openticker-runtime

Last reviewed: 2026-04-18

Runtime composition root for OpenTicker.

## Purpose

`openticker-runtime` is the long-running daemon core. It wires configuration,
connectors, normalized data, indicators, strategies, risk policy, ledger-backed
accounting, execution submission, journaling, and reconciliation into a single
runtime.

## Current Architecture

The crate uses a contextual `src/` split. `src/lib.rs` is the crate entry and
module wiring surface, while `Runtime` impl blocks are organized by behavior.

The main runtime structures are:

- `Runtime`
  Owns lanes, account config, account ledgers, shared connector registry,
  kill-switch state, observability counters, and the runtime journal.
- `LaneRuntime`
  Holds per-lane state: config clone, execution mode, lifecycle state,
  indicators, strategy engine, `BarBuilder`, risk limits, inventory and
  position state, warmup tracking, polling state, and connector-derived
  execution constraints.
- `openticker-lane`
  Owns the extracted `LaneRuntime` state model and lane-local helper DTOs while
  runtime continues to own service composition and the side-effect adapters. It
  also now hosts the extracted lane indicator/strategy construction adapter
  surface, lane bootstrap and recovered-state resolution helpers, lane
  strategy-preparation helpers, lane inventory and fill-state helpers, the pure
  signal-evaluation kernel used by the runtime planner, and the shared
  lane-cycle workflow algorithm used by `process_bar` and manual signal
  execution, plus the shared lane-polling and recovery algorithm used by
  `market_data/recovery.rs`, the shared warmup backfill algorithm used by
  `market_data/warmup.rs`, the shared execution/journaling algorithm used by
  `processing/executor.rs`, and the shared manual-close workflow used by
  `manual_ops.rs`.
- `openticker-instance`
  Owns runtime indicator and strategy assembly plus per-bar indicator evaluation.
- `openticker-gateway`
  Owns connector-registry construction and reusable connector-registry access
  while runtime keeps provider-event logging and error translation.
- `openticker-portfolio`
  Owns extracted ownership-resolution, accounting helpers, and pure
  ledger-rejection payload shaping while runtime still provides lane and
  journal views.

Current `src/` layout:

- `src/lib.rs`
  Crate root imports, module declarations, and public re-exports.
- `src/construction.rs`, `src/lifecycle.rs`, `src/manual_ops.rs`,
  `src/portfolio_adapter.rs`, `src/persistence.rs`, `src/connector_gateway.rs`,
  `src/queries/`
  Behavior-grouped `Runtime` impl blocks.
- `src/processing/`
  Signal-to-intent pipeline split into `pipeline`, `cycle`, `planner`,
  `constraints`, `executor`, `executor_engine`, `journal`, and risk rollup
  sections. The code-heavy entrypoint modules now keep their focused runtime
  logic while larger scenario tests live in sibling `*_tests.rs` files.
- `src/market_data/`
  Trade ingestion, stream payload processing, poll/dispatch, and warmup
  orchestration split by concern, including connector-facing market-data
  adapter methods, the runtime-side recovery and warmup adapters in
  `recovery_engine.rs` and `warmup_engine.rs`, and shared poll-target/bar-fetch
  helpers.
- `src/reconciliation/`
  Reconciliation orchestration split into assessment, connector snapshot, and
  apply paths with scenario-grouped tests.
- `src/model/`
  Public status/report types plus internal runtime state structs and enums.
- `src/runtime_wiring.rs`
  Runtime-focused tests around lane-owned indicator and strategy assembly.
- `src/connector_gateway.rs`
  Shared runtime adapter skeleton over `openticker-gateway`, with common
  account-kind validation, readiness checks, and runtime error translation.
- `src/repo/`
  Runtime-owned journal, bootstrap, ledger, and provider-event helper layer
  used by the thinner connector and portfolio adapters.
- `src/shared/`
  Shared runtime helpers for labels, budget math, connector/status mapping,
  symbols, inventory syncing, observability, and event logging.
- `src/errors.rs`
  `ServiceError`.

## Boot Sequence

The runtime is built through `from_config` or `from_config_with_storage`.

The storage-backed path currently does the following:

1. validate bootstrap assumptions
2. open the configured journal backend (SQLite for persistent mode)
3. build account and risk-profile lookup maps
4. build per-account ledger state from configured account budgets
5. load previous instance snapshots from the journal
6. resolve recovered lane state and build runtime lanes through `openticker-lane`
7. build the shared `ConnectorRegistry` through `openticker-gateway`
8. sync ledgers and refresh account budgets from connector snapshots
9. persist boot state
10. run startup reconciliation before marking the runtime ready

## Instance Construction

Instance construction currently derives:

- execution mode from the bound account
- risk limits from the selected risk profile plus per-instance overrides
- `BarBuilder` from lane symbol and timeframe
- indicator runtimes from `instance.indicators`
- strategy runtime from `instance.strategy`

Indicator and strategy construction now flow through `openticker-lane` /
`openticker-instance`, and lane bootstrap helpers follow the same boundary.

## Main Processing Pipeline

The central methods are `process_bar`, `process_bar_for_symbol`, and
`process_bar_for_lane` in `src/processing/pipeline.rs`.

At a high level each bar path performs:

1. kill-switch guard and lane/account consistency checks
2. connector readiness and execution-constraint loading
3. daily-loss rollover refresh
4. warmup gate handling
5. indicator evaluation and strategy intent derivation
6. order-quantity resolution and risk evaluation
7. journaling of signals, intents, risk decisions, and runtime events
8. order submission and fill/position bookkeeping if allowed
9. ledger and account-budget synchronization
10. in-memory lane state updates and observability latency tracking

## Polling And Streaming

The runtime supports both:

- polled bar fetches, using due-poll extraction and completion methods
- stream payload ingestion via `process_market_stream_payload`

Background polling orchestration currently lives in `openticker-http`, while
runtime owns due-poll bookkeeping and dispatch completion.

`src/market_data/recovery.rs` is now a runtime adapter over a lane-owned polling
and recovery algorithm, with runtime still owning connector fetches, replay,
and recovery event emission.

`src/market_data/warmup.rs` is now a thinner runtime adapter over a lane-owned
warmup algorithm, with `warmup_engine.rs` holding the runtime-side adapter and
runtime still owning history fetches, confirmed-bar replay, and warmup event
emission.

`src/market_data/dataplane.rs` now owns dataplane stream registration and
connector-backed stream history, while `dispatch.rs` owns runtime dispatch of
fetched bars and stream-level polling fan-in across matching lanes.

`src/processing/executor.rs` is now the higher-level runtime wrapper over
lane-owned execution/journaling algorithms, with `executor_engine.rs` holding
the runtime-side `LaneExecutionEngine` adapter while runtime still owns ledger
mutations, connector submission, and journal appends.

`src/processing/cycle.rs` now owns the runtime-side lane-cycle adapter and
cycle-trace persistence plumbing, so `pipeline.rs` is mostly the public
bar/manual-signal entrypoint surface.

`src/processing/constraints.rs` now owns the lane-local connector symbol
constraint initialization workflow, with runtime still owning lane mutation,
provider-event logging, and runtime event emission.

`src/manual_ops.rs` now delegates lane-local manual close sequencing to a
lane-owned workflow while runtime still owns connector access, position-record
appends, and operator-facing event emission.

`src/connector_gateway.rs` now delegates repetitive provider stage-event
assembly to a repo-level helper and no longer owns lane constraint bootstrap,
so the gateway adapter mostly focuses on runtime error mapping and connector
call sequencing.

The connector-facing methods are now domain-sliced: market-data fetch and
stream normalization live under `src/market_data/`, order submission under
`src/processing/`, and snapshot outcome logging under `src/reconciliation/`.

`src/construction.rs` now delegates journal bootstrap reads to `src/repo/` and
lane fanout/build shaping to `openticker-lane`, so it is primarily startup
orchestration plus runtime assembly.

`src/portfolio_adapter.rs` now delegates repo-backed reconciliation exception
gathering and ledger-rejection payload assembly to `src/repo/` helpers so the
adapter is mostly the mutable ledger-rejection/event-emission path, while the
read-side accounting surface plus the runtime-facing ledger snapshot/refresh
entrypoints now live under `src/repo/accounting.rs`.

## Journaling And Read Models

`Runtime` exposes journal-backed read methods for:

- runtime events
- signals
- intents
- risk decisions
- orders
- fills
- positions
- reconciliations

These methods power the HTTP control plane and the CLI dashboard.

## Reconciliation And Safety

Lifecycle safety is enforced here:

- startup reconciliation runs before readiness
- reconciliation reports are exposed per instance
- global kill switch can pause running instances
- live-mode state is surfaced through instance, connector, and service summaries

## Current State

- Runtime flow is split into contextual submodules and `src/lib.rs` is mostly
  module wiring.
- `src/connector_gateway.rs` is now a much smaller shared adapter; the
  remaining larger runtime surfaces are mostly domain modules rather than one
  monolithic connector file.
- Indicator and strategy runtime construction now live outside the runtime
  crate, reducing direct indicator-specific wiring in the composition root.
- Lane bootstrap and recovered-state resolution now live outside the runtime
  crate as well, reducing direct lane-state assembly in `src/construction.rs`.
- Strategy selection is currently limited to `single_indicator_signal` and
  `consensus`.
- In `evaluate_process_bar`, stale-data, spread, and slippage inputs are still
  placeholder values (`false` and `0`) rather than connector-driven quality
  signals.
- `processing/planner.rs` and `processing/executor.rs` are now thinner runtime
  adapters over lane-owned strategy-preparation, cycle workflow, and fill-state
  helpers.
- `manual_ops.rs` still contains operator-facing event/lifecycle behavior, but
  its lane-local close workflow now delegates to `openticker-lane`.

## Tests

Integration coverage currently includes:

- `tests/stock_paper_end_to_end.rs`
- `tests/stock_reconciliation_restart.rs`
- `tests/crypto_kline_ingestion.rs`

## Verify

- `cargo test -p openticker-runtime`
