# openticker-strategy

Last reviewed: 2026-04-18

Trade-intent mapping logic for OpenTicker.

## Purpose

`openticker-strategy` turns indicator outputs into venue-neutral trade intents while keeping V1 semantics long-only and spot-focused.

## Current Architecture

The crate is small and split into focused source modules.

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Crate entrypoint and public re-exports |
| `src/context.rs` | Shared strategy input contexts |
| `src/decision.rs` | `StrategyDecision` result model |
| `src/traits.rs` | Strategy contracts |
| `src/single_indicator.rs` | Single-indicator long-only implementation |
| `src/consensus.rs` | Consensus long-only implementation |
| `src/metadata.rs` | Shared metadata-filtering helpers |
| `src/tests.rs` | Unit coverage for mapping and policy behavior |

There are two strategy styles today:

- `SingleIndicatorLongOnlyStrategy`
- `ConsensusLongOnlyStrategy`

The shared input models are:

- `StrategyContext`
- `IndicatorObservation`
- `ConsensusStrategyContext`

## How It Works

### Single-indicator mode

`SingleIndicatorLongOnlyStrategy` maps one representative `IndicatorSignal` into a `TradeIntent`.

- buy signal without position -> `OpenLong`
- buy signal with position -> `AddLong`
- sell signal with position -> `CloseLong`
- sell signal without position -> `NoOp`
- no signal -> `NoOp`

### Consensus mode

`ConsensusLongOnlyStrategy` uses weighted observations.

- primary-signal indicators contribute directional score
- filter indicators can veto a direction
- preview signals only count when `IndicatorSignalPolicy::PreviewAllowed` permits them
- the final direction is translated into long-only intents

## Current State

- Only two strategies exist.
- The crate maps signals to intents but does not own cooldowns, journaling, or risk checks.
- There is no structured explanation payload yet; decisions are still implicit in the input observations and resulting intent.

## Refactor Notes

- If richer strategy explainability is needed, this crate is the right place for a strategy-decision record type.
- Keep long-only mapping explicit unless the global trading model expands.

## Verify

- `cargo test -p openticker-strategy`
