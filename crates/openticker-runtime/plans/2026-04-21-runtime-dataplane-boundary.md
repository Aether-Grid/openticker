# Runtime / Dataplane Boundary Plan

Last reviewed: 2026-04-21

## Goal

Make the `openticker-runtime` and `openticker-dataplane` market-data boundary
explicit enough that:

- `openticker-dataplane` clearly owns stream scheduling, due-stream selection,
  per-stream buffering, and fetch-attempt bookkeeping
- `openticker-runtime` clearly owns lane advancement, warmup/recovery
  decisions, connector-aware fetches, and signal/execution side effects
- the runtime background polling path becomes thin glue instead of a
  second scheduler implementation

This is the next major refactor after the runtime crate-internal splits. It is
not another file-shuffle pass. It is a subsystem-boundary cleanup across
`openticker-runtime`, `openticker-dataplane`, and the HTTP wiring that owns the
shared `DataPlane`.

## Why This Is Next

The remaining large runtime surfaces are mostly legitimate orchestration, but
market-data ownership is still distributed across too many layers:

- `crates/openticker-runtime/src/polling_supervisor.rs`
- `crates/openticker-runtime/src/market_data/polling.rs`
- `crates/openticker-runtime/src/market_data/dispatch.rs`
- `crates/openticker-runtime/src/market_data/gateway.rs`
- `crates/openticker-runtime/src/market_data/recovery_engine.rs`
- `crates/openticker-runtime/src/market_data/warmup_engine.rs`
- `crates/openticker-http/src/state.rs`
- `crates/openticker-http/src/handlers.rs`

The largest remaining ambiguity is not “who owns this helper,” but “who owns
the market-data control flow.” Right now:

- `openticker-dataplane` already has `DataPlane::run_forever`
- runtime still has `RuntimePollingSupervisor` with its own loop
- HTTP owns the shared `DataPlane` instance and stream replacement
- runtime owns stream selection and lane advancement

That is workable, but it is not a crisp boundary.

## Non-Goals

This campaign should not:

- move connector fetch logic into `openticker-dataplane`
- move lane strategy/risk/execution logic into `openticker-dataplane`
- add persistence to `openticker-dataplane`
- merge runtime warmup or recovery state machines into the dataplane crate
- redesign operator-facing HTTP payloads unless necessary for the boundary

Those would break existing crate roles.

## Target End State

The desired shape is:

- `openticker-dataplane`
  - owns stream registry replacement
  - owns due-stream selection
  - owns polling attempt bookkeeping
  - owns in-memory bar buffering and stream health snapshots
  - owns the generic polling loop entrypoint
- `openticker-runtime`
  - exposes a narrow market-data consumer boundary for “advance this stream”
  - returns the bars that should be recorded into the dataplane buffer
  - owns lane dispatch, warmup/recovery advancement, and connector-aware fetch
    policy
  - does not open-code a separate scheduler loop
- `openticker-http`
  - continues to own process-level startup and the shared `DataPlane`
  - wires runtime and dataplane together through a smaller, clearer surface

In practice, runtime should feel like a consumer of due streams, not a peer
scheduler.

## Proposed Runtime Boundary

Introduce a dedicated runtime-side market-data service boundary, likely centered
on one narrow adapter rather than scattered `Runtime` impl methods.

The shape should be close to:

- `desired_stream_specs() -> Vec<StreamSpec>`
- `advance_stream(key: &StreamKey, now_ms: i64) -> Result<MarketDataAdvance, ServiceError>`
- `dispatch_buffered_bar(key: &StreamKey, bar: &OhlcvBar) -> Result<Vec<ProcessBarOutcome>, ServiceError>`

Where `MarketDataAdvance` is runtime-owned and explicit about what dataplane may
record:

- `recorded_bars: Vec<OhlcvBar>`
- `outcomes: Vec<ProcessBarOutcome>`
- optional future metadata only if truly needed for operator surfaces

This does not need to become a new crate. It just needs to stop being spread
across unrelated runtime modules.

## Implementation Phases

### Phase 1: Converge On One Scheduler

Replace the custom loop in
`crates/openticker-runtime/src/polling_supervisor.rs` with a thinner wrapper
over `openticker-dataplane` scheduling primitives.

Preferred direction:

- reuse `DataPlane::run_forever` instead of keeping two scheduling loops
- move runtime-specific behavior into injected callbacks or a small adapter
- keep process lifecycle ownership in runtime/HTTP, not in dataplane

Expected result:

- `RuntimePollingSupervisor` becomes a thin startup/shutdown wrapper
- dataplane owns the scheduling loop semantics
- runtime only owns what happens when a due stream is consumed

### Phase 2: Collapse Runtime Market-Data Entry Surface

Create a dedicated runtime market-data adapter module that becomes the single
owner of stream advancement.

Likely files touched:

- `crates/openticker-runtime/src/market_data/polling.rs`
- `crates/openticker-runtime/src/market_data/dispatch.rs`
- `crates/openticker-runtime/src/market_data/gateway.rs`
- `crates/openticker-runtime/src/market_data/recovery_engine.rs`
- `crates/openticker-runtime/src/market_data/warmup_engine.rs`

Tasks:

- gather `advance_stream_polling_once(...)` and adjacent helpers under one
  explicit adapter surface
- keep `process_trade(...)` and `process_market_stream_payload(...)` separate,
  because they are ingestion entrypoints, not scheduler flow
- keep recovery and warmup engines as internal collaborators, not external
  scheduler owners

Expected result:

- one clear runtime module owns due-stream consumption
- `dispatch.rs` and `polling.rs` become narrower entrypoint helpers
- stream advancement stops feeling spread across half a dozen files

### Phase 3: Clarify Stream Ownership Between Runtime And HTTP

Reduce the number of places that know how to compute or replace stream specs.

Current tension:

- runtime computes `effective_streams_for_dataplane()`
- HTTP owns `Arc<DataPlane>` creation and replacement
- runtime tests and supervisor also interact with `DataPlane`

Plan:

- keep runtime as the owner of desired stream specs
- keep HTTP as the owner of the shared `DataPlane`
- expose one obvious runtime method for “current desired stream set”
- use that same method everywhere stream replacement happens

Expected result:

- no duplicated stream-assembly path
- less ad hoc coupling between HTTP handlers, supervisor tests, and runtime

### Phase 4: Tighten Cross-Crate Contracts

After the runtime-side adapter exists, decide whether any public dataplane API
should be simplified for this use case.

Potential refinements:

- small naming cleanup around “manual poll attempt” vs scheduler-driven attempt
- a narrower success/failure callback contract for `run_forever`
- possibly a typed completion payload if the callback shape is too stringly

Constraint:

- keep `openticker-dataplane` runtime-neutral and serialization-friendly

Do not add runtime-specific types to the dataplane crate.

### Phase 5: Re-Test The Full Polling Story

This boundary touches live control flow, so verification needs to go beyond unit
tests.

Minimum expected verification:

- `cargo test -p openticker-dataplane`
- `cargo test -p openticker-runtime`
- `cargo test -p openticker-http`

High-value paths to keep green:

- background polling startup/shutdown
- dataplane stream replacement after lifecycle changes
- duplicate confirmed-bar dedupe
- warmup-before-running behavior
- recovery replay and auto-resume behavior
- HTTP stream status and metrics endpoints

## Suggested File Sequence

Implement in this order to keep the refactor incremental:

1. `crates/openticker-runtime/src/polling_supervisor.rs`
2. `crates/openticker-runtime/src/market_data/` runtime-side adapter module
3. `crates/openticker-http/src/state.rs`
4. `crates/openticker-http/src/handlers.rs`
5. targeted `openticker-dataplane` callback/API cleanup if the runtime adapter
   exposes friction

That keeps the risky behavior changes late and the local runtime extraction
early.

## Exit Criteria

This campaign is done when:

- runtime no longer open-codes a competing scheduler loop
- one runtime-owned market-data adapter owns due-stream consumption
- HTTP only wires together runtime and the shared dataplane instance
- stream registration/replacement flows through one obvious runtime method
- background polling, warmup, recovery, and stream-status tests stay green

## What Should Wait Until After This

Once this is finished, the next major runtime campaign should be the remaining
processing orchestration boundary:

- `crates/openticker-runtime/src/processing/cycle.rs`
- `crates/openticker-runtime/src/processing/planner.rs`
- `crates/openticker-runtime/src/processing/executor_engine.rs`

That is the other subsystem that still has legitimate runtime density after the
crate-boundary cleanup.
