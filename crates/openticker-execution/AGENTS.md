# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate defines the workspace's venue-neutral execution request and acceptance contracts.

## Package And Commands

- Cargo package: `openticker-execution`
- Crate entry (public re-exports): `src/lib.rs`
- Implementation modules: `src/types.rs`, `src/error.rs`, `src/intent.rs`, `src/sizing.rs`, `src/router.rs`
- Verify: `cargo test -p openticker-execution`

## Current Working Shape

- Only market orders are modeled.
- `PaperExecutionRouter` is the only concrete router here.
- `stable_client_order_id` and `order_side_for_intent` are important shared helpers.
- Unit tests are split between intent helpers (`src/intent.rs`), sizing (`src/sizing.rs`), and router behavior (`src/router.rs`).

## Invariants

- Keep the API venue-neutral.
- Preserve deterministic client order ID generation unless coordinated downstream.
- Keep long-only spot semantics explicit when mapping intents to order side.

## Common Change Recipes

### Add a new order field or order type

1. Update `ExecutionRequest`, `AcceptedOrder`, or enums in `src/types.rs`.
2. Update the paper router behavior in `src/router.rs`.
3. Update downstream runtime, connector, and storage callers if the shape changed.
4. Add focused tests for the new mapping or validation rule.

## Watchouts

- `NoOp` must remain non-executable unless the wider model changes.
- Client order ID changes can affect reconciliation and duplicate-submission behavior.

## Common Follow-Ups

- Update `crates/openticker-runtime` and `crates/openticker-connectors` if request or accepted-order shapes change.
