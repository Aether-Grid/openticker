# openticker-connectors

Last reviewed: 2026-04-18

Venue and account connector adapters for OpenTicker.

## Purpose

`openticker-connectors` is the external integration layer for market data, execution, account snapshots, stream normalization, and connector capability metadata.

## Current Architecture

The crate is split into:

- `src/lib.rs`
  Crate entrypoint and re-export surface.
- `src/types.rs`, `src/traits.rs`, `src/error.rs`
  Shared connector domain types, contracts, and error surface.
- `src/capabilities.rs`, `src/helpers.rs`, `src/stub.rs`, `src/registry.rs`
  Capability metadata, shared utility helpers, stub connector behavior, and registry orchestration.
- `src/connectors/alpaca/`
  Alpaca connector, REST decoding, account/bar/order normalization, and tests.
- `src/connectors/binance/`
  Binance connector, signing, REST and stream decoding, snapshot/kline/order normalization, and tests.

The registry is account-centric, not connector-kind-centric. Each configured account gets one registry entry and one concrete connector client.

## Connector Contracts

The current shared interfaces include:

- health reporting
- account snapshot reconciliation
- latest-bar market data
- execution submission
- symbol-constraint lookup
- market stream normalization
- private stream normalization
- runtime control hooks for disconnect and reconnect bookkeeping

These are combined behind the internal `ConnectorClient` trait and stored in `ConnectorRegistry`.

## How It Works

1. `ConnectorRegistry::from_accounts` builds one concrete connector per account.
2. Each entry stores the `ConnectorAccount`, a concrete client, and any mode-validation error.
3. Registry methods such as `fetch_latest_bar`, `submit_order`, and stream normalization first gate on connector health.
4. Reconciliation-oriented methods like `fetch_account_snapshot_unchecked` and `fetch_symbol_constraints_unchecked` deliberately bypass the connected-state requirement.
5. Adapters normalize all venue-specific payloads into OpenTicker-owned types before values leave this crate.

## Capability And Resilience Model

The crate records connector capabilities and operational expectations through:

- `ConnectorDescriptor`
- `ConnectorResiliencePolicy`
- `ConnectorResilienceState`
- `connector_matrix()`

This metadata describes roles, paper/live/demo support, reconnect policy, throttling behavior, and connection-shape expectations.

## Current State

- Supported connectors are currently `alpaca` and `binance` only.
- The crate exposes deterministic remote client order ID helpers so venue adapters can avoid duplicate client IDs.
- Blocking `reqwest` clients are initialized on a dedicated OS thread and then shared through a process-wide `OnceLock` to avoid runtime-context issues.
- Health gating is strict for normal operations, but reconciliation paths intentionally allow some degraded cases.

## Refactor Notes

- Adding a new connector currently requires touching both this crate and `openticker-config`, because connector capability knowledge is duplicated.
- `connector_matrix()` is the public capability summary, but runtime wiring still depends on explicit construction branches in `ConnectorRegistry::from_accounts`.
- Each venue adapter is split by responsibility under `src/connectors/alpaca/` and `src/connectors/binance/`.

## Verify

- `cargo test -p openticker-connectors`
