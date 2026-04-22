# openticker-dataplane

Last reviewed: 2026-04-18

Always-on market-data stream registry, polling scheduler, in-memory bar buffers, and stream health snapshots.

## Purpose

`openticker-dataplane` keeps the data plane separate from bot lifecycle. It owns the stream universe, due-stream selection, bounded bar retention, and per-stream status so HTTP and runtime layers can share one fetch path per `(account, symbol, timeframe)`.

## Current Architecture

The crate currently revolves around five pieces:

- `StreamKey`, `StreamSource`, and `StreamSpec` define the logical stream universe.
- `StreamRegistry` merges stream specs by key and keeps one logical stream per account, symbol, and timeframe.
- `StreamBuffer` retains recent bars and derives sparklines.
- `DataPlane` owns registry state, in-memory buffers, status bookkeeping, and metrics.
- `DataPlane::run_forever` is the generic async polling loop that stays connector-agnostic through injected callbacks.

Internal responsibilities are now split by concern:

- `src/stream.rs`: stream keys/specs/status models and key ordering.
- `src/registry.rs`: stream spec merge/dedup registry behavior.
- `src/buffer.rs`: bounded in-memory bar retention and sparkline snapshots.
- `src/metrics.rs`: dataplane runtime counters and latency snapshots.
- `src/dataplane.rs`: orchestrator state, polling lifecycle, and `run_forever` loop.
- `src/lib.rs`: crate facade and public re-exports.

## Public Surface

The most important public methods are:

- `DataPlane::replace_streams`
- `DataPlane::registered_streams`
- `DataPlane::take_due_streams`
- `DataPlane::record_fetched_bar`
- `DataPlane::record_fetch_error`
- `DataPlane::record_manual_poll_attempt`
- `DataPlane::snapshot_bars`
- `DataPlane::snapshot_streams`
- `DataPlane::metrics_snapshot`
- `DataPlane::run_forever`

## How It Works

1. `openticker-runtime` computes the desired `StreamSpec` set.
2. `openticker-http` installs that set into `DataPlane` with `replace_streams`.
3. `take_due_streams` marks due streams and increments fetch counters.
4. Callers fetch the newest bar and hand results back through `record_fetched_bar` or `record_fetch_error`.
5. `snapshot_streams` and `metrics_snapshot` expose current operator-facing state.
6. `run_forever` wires those pieces together with attempt, fetch, success, and error callbacks while staying venue-neutral.

## Current State

- Bars stay in memory only.
- `fetch_count` increments when a stream becomes due, not only on successful remote fetch.
- `record_fetched_bar` updates success timing even when the bar is not newer than retained history.
- `run_forever` lives here, but `openticker-http` owns process-level startup and callback wiring.

## Refactor Notes

- Keep connector fetch logic and runtime signal evaluation outside this crate.
- Public snapshots should stay serialization-friendly because HTTP and dashboard surfaces depend on them directly.
- Keep module boundaries aligned to domain concerns (`stream`, `registry`, `buffer`, `metrics`, `dataplane`) as behavior grows.

## Verify

- `cargo test -p openticker-dataplane`
