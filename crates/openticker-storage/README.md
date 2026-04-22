# openticker-storage

Last reviewed: 2026-04-18

Runtime journaling and persistence backends for OpenTicker.

## Purpose

`openticker-storage` stores runtime history and restart state. It is the audit log and recovery layer for signals, intents, risk decisions, orders, fills, positions, reconciliation records, lifecycle events, and bot snapshots.

## Current Architecture

The crate currently has three main pieces:

- a large `RuntimeJournal` trait that defines the full persistence contract
- `OperatorReadModels`, which projects recent journal writes into in-memory operator feeds
- `InMemoryRuntimeJournal`, used for lightweight runtime construction and tests
- `SqliteRuntimeJournal`, used for durable runtime storage

The crate is organized into a small set of focused modules:

- `src/lib.rs` for shared record and write models, the `RuntimeJournal` trait, and shared helpers
- `src/operator_read_models.rs` for in-memory projected operator query state
- `src/in_memory_impl.rs` for `InMemoryRuntimeJournal`
- `src/sqlite_impl.rs` for `SqliteRuntimeJournal`
- `src/sqlite_migrations.rs` for embedded SQLite schema initialization and schema-version checks
- `src/tests.rs` for crate tests

## Data Model

The journal contract currently covers:

- runtime events
- signals
- intents
- risk decisions
- orders
- fills
- positions
- reconciliations
- bot events
- service events
- bot snapshots

Each family has a persisted `*Record` type and a write-side `*Write` type.

## How It Works

- The runtime appends write-side records through `RuntimeJournal` methods.
- Read-side methods such as `recent_orders`, `recent_risk_decisions`, and `latest_reconciliation_for_bot` are used to drive operator inspection and recovery flows.
- The in-memory backend stores data in mutex-protected vectors and maps.
- The SQLite backend opens one write connection plus a small read-only connection pool, enables WAL mode, initializes schema, and routes reads/writes through separate paths.

## SQLite Notes

`SqliteRuntimeJournal` currently:

- creates parent directories when needed
- enables `journal_mode = WAL`
- enables `synchronous = NORMAL`
- tracks schema state with `PRAGMA user_version`
- initializes the current built-in schema for fresh databases
- rejects incompatible on-disk schema versions instead of migrating them in place

## Current State

- Idempotency is currently enforced explicitly for orders and fills.
- The SQLite backend uses one mutex-guarded write `Connection` and a small round-robin pool of read-only connections.
- The crate is functionally broad and now split by backend modules, but each backend module remains large.
- SQLite uses one embedded current schema; older on-disk schema versions are not migrated in place.

## Refactor Notes

- Any schema change must be reflected in both the SQL schema and the query and insert code paths.
- If a persisted shape changes, expect downstream effects in runtime recovery, HTTP inspection endpoints, and CLI output.
- The trait surface is already large enough that future modularization by record family may be useful.

## Verify

- `cargo test -p openticker-storage`
