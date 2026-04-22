# AGENTS.md

Last reviewed: 2026-04-19

## Overview

This crate owns the typed cycle-trace read model for operator inspection.

## Package And Commands

- Cargo package: `openticker-trace`
- Entry file: `src/lib.rs`
- Verify: `cargo test -p openticker-trace`

## Invariants

- Keep this crate pure: no runtime orchestration, storage backends, HTTP types, or connector I/O.
- Prefer stable serialized DTOs over runtime-specific internal structs.
- Keep identity and capital helpers reusable by both HTTP and future CLI/TUI consumers.

## Common Change Recipes

### Add a new trace field

1. Update the DTO in this crate.
2. Keep serde behavior explicit when the field is optional.
3. Update runtime assembly and storage serialization together.

### Change trace identity behavior

1. Update `src/id.rs`.
2. Keep generated IDs opaque and additive.
3. Re-test any runtime or HTTP code that persists or routes by `trace_id`.
