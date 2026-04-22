# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate owns always-on market-data stream scheduling, stream deduplication, in-memory bar retention, and per-stream health snapshots.

## Package And Commands

- Cargo package: `openticker-dataplane`
- Entry facade: `src/lib.rs`
- Core modules: `src/stream.rs`, `src/registry.rs`, `src/buffer.rs`, `src/metrics.rs`, `src/dataplane.rs`
- Tests: `src/tests.rs`
- Verify: `cargo test -p openticker-dataplane`

## Current Working Shape

- `StreamRegistry` merges stream specs by `(account, symbol, timeframe)`.
- `StreamBuffer` is the bounded in-memory retention layer.
- `DataPlane` owns registry state, buffers, status bookkeeping, and metrics behind an internal mutex.
- `run_forever` is the generic async scheduler; `openticker-http` injects the actual attempt, fetch, success, and error callbacks.
- `src/lib.rs` stays as a thin public re-export facade.

## Invariants

- Keep the crate venue-neutral and runtime-neutral.
- Keep bars in memory only; do not add persistence here.
- Preserve one logical stream per `(account, symbol, timeframe)` key.
- Prefer deterministic, synchronous core APIs so tests can drive time directly.

## Common Change Recipes

### Add a new stream snapshot or metrics field

1. Update the public snapshot type.
2. Populate it in `dataplane::StreamEntry::status` or the metrics snapshot path.
3. Keep the shape serialization-friendly.
4. Re-check downstream HTTP and dashboard consumers.

### Change polling semantics

1. Review `take_due_streams`, `record_manual_poll_attempt`, `record_fetched_bar`, `record_fetch_error`, and `run_forever` together.
2. Keep fetch and completion counters internally consistent.
3. Re-test the HTTP integration path that drives the dataplane loop.

### Change retention behavior

1. Update `StreamBuffer` deliberately.
2. Add tests for duplicate or older bars, trimming, and sparkline output.
3. Keep the latest-bar and snapshot ordering stable.

## Watchouts

- HTTP and runtime both depend on these types, so keep the public surface small and serialization-friendly.
- `fetch_count` tracks due-poll attempts, not only successful bar appends.
- Do not move connector fetch logic or bot signal evaluation into this crate.
- Keep `src/lib.rs` re-exports updated when adding or moving public types.
