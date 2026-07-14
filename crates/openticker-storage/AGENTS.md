# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate is the runtime journal and persistence layer. It is the source of audit history and restart state.

## Package And Commands

- Cargo package: `openticker-storage`
- Entry file: `src/lib.rs`
- Backend files: `src/in_memory.rs`, `src/sqlite/`
- Verify: `cargo test -p openticker-storage`

## Current Working Shape

- `RuntimeJournal` is the central abstraction.
- `OperatorReadModels` is the in-memory projected read side for operator queries.
- `InMemoryRuntimeJournal` mirrors the trait with mutex-protected in-memory collections.
- `SqliteRuntimeJournal` mirrors the same trait over one WAL-enabled write SQLite connection plus a small read-connection pool.
- Record and write types live in `src/records.rs`; the journal trait lives in `src/journal.rs`.

## Invariants

- Keep record families and journal trait methods aligned.
- Preserve idempotent order and fill insertion behavior unless intentionally changed.
- Keep SQLite schema, schema-version checks, inserts, and reads synchronized.

## Common Change Recipes

### Add a new persisted record family

1. Add `*Record` and `*Write` types.
2. Extend `RuntimeJournal`.
3. Implement the new methods in both in-memory and SQLite backends.
4. Update the SQLite schema.
5. Add tests for both backends.

### Change an existing record shape

1. Update type definitions.
2. Update inserts and queries in both backends.
3. Update schema and the supported schema version if needed.
4. Update runtime, HTTP, and CLI consumers.

## Watchouts

- Schema changes can affect recovery and reconciliation behavior even when tests still compile.
- Backend modules are still large; avoid partial changes that update only one backend.

## Common Follow-Ups

- Update `crates/openticker-runtime` when persistence semantics or restart assumptions change.
- Update `crates/openticker-http` and `crates/openticker-cli` if operator-facing record shapes change.
