# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate provides a runtime-agnostic facade over connector-registry
operations.

`openticker-gateway` should stay focused on:

- connector status/readiness access
- readiness-gated market-data calls
- readiness-gated execution submission
- gateway-level error normalization around connector-registry operations

## Package And Commands

- Cargo package: `openticker-gateway`
- Main file: `src/lib.rs` (module index; implementation lives in `src/gateway.rs`,
  `src/error.rs`, `src/registry.rs`, and `src/constraints.rs`)
- Verify: `cargo test -p openticker-gateway`

## Current Working Shape

- `Gateway` wraps `Arc<Mutex<ConnectorRegistry>>`.
- `GatewayError` normalizes lock, unknown-account, readiness, and connector
  errors.
- Readiness-gated methods call `ensure_account_ready(...)` before market data
  and order-submission operations.
- `fetch_account_snapshot_unchecked(...)` and
  `fetch_symbol_constraints_unchecked(...)` are intentionally non-gated helper
  paths.

## Invariants

- Keep unknown-account handling explicit and stable.
- Preserve readiness gating for live data and execution operations.
- Keep this crate runtime-agnostic: no journal writes, operator messaging, or
  runtime observability logic belongs here.

## Common Change Recipes

### Add a new connector operation

1. Add the gateway method in `src/gateway.rs`.
2. Decide whether readiness-gating is required for the operation.
3. Map connector errors into `GatewayError` variants consistently.
4. Update runtime call sites in `crates/openticker-runtime/src/connector_gateway.rs`.

### Change readiness semantics

1. Update `ensure_account_ready(...)` behavior.
2. Keep reconnect/disconnect bookkeeping behavior intentional.
3. Re-verify all methods that currently call readiness checks before connector
   operations.

## Watchouts

- The connector-registry mutex is held while calling through registry methods.
- `ConnectorNotReady` reason text is currently assembled as a comma-delimited
  string.

## Common Follow-Ups

- Update `crates/openticker-runtime/src/connector_gateway.rs` error mapping when
  `GatewayError` changes.
- Update runtime readiness and provider-event tests when gateway semantics
  change.
