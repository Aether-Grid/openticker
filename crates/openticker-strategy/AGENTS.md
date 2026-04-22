# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This crate maps indicator outputs into normalized trade intents. It should stay focused on decision policy, not execution or persistence.

## Package And Commands

- Cargo package: `openticker-strategy`
- Entry file: `src/lib.rs` (re-export and module wiring)
- Implementation modules: `src/context.rs`, `src/decision.rs`, `src/traits.rs`, `src/single_indicator.rs`, `src/consensus.rs`, `src/metadata.rs`
- Verify: `cargo test -p openticker-strategy`

## Current Working Shape

- `SingleIndicatorLongOnlyStrategy` handles one representative signal.
- `ConsensusLongOnlyStrategy` handles weighted multi-indicator decisions with filter veto behavior.
- Input types are intentionally lightweight and runtime-agnostic.
- Tests live in `src/tests.rs` and cover single-indicator and consensus behavior.

## Invariants

- Preserve long-only spot mapping unless the workspace model changes.
- Keep preview-policy gating explicit in consensus logic.
- Do not move risk checks, cooldown state, or journal side effects into this crate.

## Common Change Recipes

### Add a new strategy

1. Add the strategy type and its input context here.
2. Add focused tests for the mapping behavior.
3. Update `openticker-runtime` so instances can construct and use it.

### Change consensus behavior

1. Review `signal_vote`, threshold handling, and filter veto logic together.
2. Update tests for buy, sell, veto, and preview-policy behavior.

## Watchouts

- Strategy changes frequently alter runtime behavior without changing any outer API shape, so integration tests matter.

## Common Follow-Ups

- Update `crates/openticker-runtime` if new strategies or new context fields are introduced.
