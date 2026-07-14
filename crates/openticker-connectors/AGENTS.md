# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This directory is the venue-integration boundary for the workspace. Everything here should normalize external behavior into OpenTicker-owned types before values leave the crate.

## Package And Commands

- Cargo package: `openticker-connectors`
- Main files: `src/lib.rs`, `src/registry.rs`, `src/connectors/alpaca/`, `src/connectors/binance/`
- Verify: `cargo test -p openticker-connectors`

## Current Working Shape

- `ConnectorRegistry` is keyed by account ID.
- Concrete adapters are selected in `ConnectorRegistry::from_accounts`.
- `connector_matrix()` is the public capability summary.
- Normal operation methods gate on health; some reconciliation helpers intentionally bypass that gate.
- Shared blocking HTTP client creation is isolated from Tokio runtime context on purpose.

## Invariants

- Do not leak venue SDK or raw payload types outside this crate.
- Keep connector capability metadata consistent with actual adapter behavior.
- Preserve deterministic remote client order ID behavior unless the change is coordinated with runtime and storage.
- Reconnection and throttling policy belong here, not in `openticker-runtime`.

## Common Change Recipes

### Add a new connector

1. Add a new adapter module.
2. Implement the shared connector traits.
3. Wire construction into `ConnectorRegistry::from_accounts`.
4. Add a descriptor in `descriptor_for` and `connector_matrix()`.
5. Update `openticker-config` so validation knows about the new connector kind.
6. Add runtime-facing tests for polling, execution, or stream normalization as needed.

### Change connector payload normalization

1. Update the adapter's normalization function.
2. Keep normalized outputs in terms of `openticker-core`, `openticker-data`, and `openticker-execution` types only.
3. Update any runtime ingestion tests that depend on payload shape or dedupe behavior.

## Watchouts

- Some registry operations have checked and unchecked variants. Use the unchecked variants only for reconciliation-style flows.
- Health-state transitions and resilience fields affect operator status surfaces downstream.

## Common Follow-Ups

- Update `crates/openticker-config` when connector kinds, secrets, or market support change.
- Update `crates/openticker-runtime` tests when connector behavior changes affect polling, reconciliation, or stream handling.
