# AGENTS.md

Last reviewed: 2026-07-14

## Overview

This crate owns per-lane runtime state and lane-local model types extracted from
`openticker-runtime`.

It should stay focused on:

- lane lifecycle and recovery state enums
- lane runtime state storage
- lane identity, recovered-state resolution, and lane bootstrap helpers
- lane warmup and reconciliation helper DTOs
- lane-local process evaluation DTOs
- lane indicator and strategy construction adapters
- lane strategy-preparation helpers
- pure lane inventory, ownership, fill-state, and signal-evaluation helpers
- lane-cycle workflow orchestration that is expressed through injected runtime
  ports rather than direct connector or journal access
- lane warmup and polling workflow orchestration that is expressed through
  injected runtime ports rather than direct connector or journal access
- lane execution and journaling workflow orchestration that is expressed through
  injected runtime ports rather than direct connector or journal access
- lane manual-close workflow orchestration that is expressed through injected
  runtime ports rather than direct connector or journal access

## Package And Commands

- Cargo package: `openticker-lane`
- Public facade: `src/lib.rs`
- Implementation modules: `src/{build,cycle,execution,manual_ops,polling,position,reconcile,recovery,signals,state,trace,warmup}.rs`
- Unit tests: `src/tests.rs`
- Verify: `cargo test -p openticker-lane`

## Invariants

- Keep this crate free of runtime composition concerns.
- Do not move connector registry, journal, or service orchestration here.
- Lane-local state mutation is allowed here when it is pure and does not touch
  runtime journals, connector clients, or service lifecycle orchestration.
- Side-effecting lane workflows may live here only when they operate through
  abstract engine ports and do not own runtime composition directly.

## Common Follow-Ups

- Update `crates/openticker-runtime` when lane state or helper contracts change.
- Update `crates/openticker-http` and `crates/openticker-cli` if lane-facing API
  types are eventually re-exported there.
