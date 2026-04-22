# openticker-risk

Last reviewed: 2026-04-18

Risk policy primitives for OpenTicker.

## Purpose

`openticker-risk` evaluates whether an intent may proceed based on configured limits and current runtime context.

## Current Architecture

The crate is intentionally simple and organized across a small set of files:

- `src/lib.rs` for module wiring and public re-exports
- `src/types.rs` for risk inputs and decision types
- `src/policy.rs` for policy traits and concrete policy evaluation
- `src/tests.rs` for unit coverage of policy behavior

The core pieces are:

- `RiskLimits`
- `RiskContext`
- `RiskDecision`
- `RiskPolicy`
- `BasicRiskPolicy`

`BasicRiskPolicy` is the only concrete policy implementation today.

## How It Works

`BasicRiskPolicy::evaluate` in `src/policy.rs` currently checks, in order:

1. global kill switch
2. basic positive price and quantity validation for executable intents
3. pass-through allowance for non-opening intents after basic validation
4. cooldown state
5. stale-data state
6. spread and slippage limits
7. daily loss limit
8. per-order notional limit
9. open-position count limit

If all checks pass, the original intent is returned inside `RiskDecision::Allow`.

## Current State

- The decision model is binary: `Allow` or `Reject`.
- There is no modify-or-clamp decision type here yet.
- Reduce and close intents are intentionally easier to pass through than open and add intents.
- Runtime still supplies placeholder market-quality inputs in some paths, so not every check is fully live-sourced yet.
- This crate is pure and has no storage, HTTP, or connector dependencies.

## Refactor Notes

- If risk needs structured machine-readable reasons instead of static strings, this crate is where that model should start.
- Keep side effects out of this crate. Journaling and operator visibility belong in outer layers.

## Verify

- `cargo test -p openticker-risk`
