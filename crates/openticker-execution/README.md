# openticker-execution

Last reviewed: 2026-04-18

Venue-neutral execution contracts for OpenTicker.

## Purpose

`openticker-execution` defines the minimal order-submission contract between the runtime and the execution layer. It keeps the rest of the workspace from depending on venue-specific order APIs.

## Current Architecture

The crate is intentionally compact and currently provides:

- order-side and order-type enums
- `ExecutionRequest`
- `AcceptedOrder`
- `OrderQuantityResolution`
- `ExecutionRouter`
- `PaperExecutionRouter`
- deterministic helper functions for order-side mapping, client order IDs, and
  order-quantity resolution under exchange constraints

The public surface remains in `src/lib.rs`, and implementation is split into focused modules:

- `src/types.rs` for request/response and order enums
- `src/error.rs` for `ExecutionError`
- `src/intent.rs` for intent mapping, deterministic client-order IDs, and their tests
- `src/sizing.rs` for quantity resolution under market and venue constraints
- `src/router.rs` for the router trait, paper router, and router tests

## How It Works

- `ExecutionRequest` carries the normalized request the runtime wants to submit.
- `order_side_for_intent` converts `TradeIntent` into `Buy` or `Sell` and rejects `NoOp`.
- `stable_client_order_id` derives a deterministic client order ID from instance, timestamp, and intent.
- `PaperExecutionRouter` validates price and quantity and returns a synthetic accepted order.

## Current State

- Only `Market` orders are modeled.
- Only a paper router is implemented in this crate.
- Cancel, amend, and richer venue-neutral order lifecycle contracts are not here yet.

## Refactor Notes

- If order modeling expands, keep this crate venue-neutral and push exchange-specific behavior into `openticker-connectors`.
- Any change to client order ID generation can affect reconciliation, storage, and duplicate-submission behavior.
- Keep `src/lib.rs` as a thin re-export surface so downstream crates are insulated from module layout changes.

## Verify

- `cargo test -p openticker-execution`
