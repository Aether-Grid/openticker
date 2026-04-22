# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate is the pure risk-evaluation layer. It should stay small, explicit, and side-effect free.

## Package And Commands

- Cargo package: `openticker-risk`
- Main files: `src/lib.rs`, `src/types.rs`, `src/policy.rs`
- Verify: `cargo test -p openticker-risk`

## Current Working Shape

- `BasicRiskPolicy` is the only concrete policy.
- Decisions are currently binary: `Allow` or `Reject`.
- Opening and adding long positions receive the full limit checks; reducing and closing are effectively pass-through after basic validation.
- Public surface remains re-exported from `src/lib.rs`.

## Invariants

- Keep evaluation pure. No storage, logging side effects, HTTP calls, or connector logic belong here.
- Preserve explicit long-only assumptions unless the workspace model changes.
- Reject reasons should stay stable and understandable to operators.

## Common Change Recipes

### Add a new risk check

1. Add fields to `RiskLimits` or `RiskContext` if needed.
2. Insert the new evaluation step in `BasicRiskPolicy::evaluate` deliberately.
3. Add focused tests for pass and fail behavior.
4. Update runtime construction if the new fields need to be populated from config or market state.

## Watchouts

- Order of checks matters because reject reasons are user-visible and persisted downstream.
- If you add new context fields, `openticker-runtime` must populate them.

## Common Follow-Ups

- Update `crates/openticker-runtime` when `RiskContext` or `RiskLimits` change.
