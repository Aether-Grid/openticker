# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This directory contains the smallest shared domain contracts in the workspace. Changes here propagate widely.

## Package And Commands

- Cargo package: `openticker-core`
- Entry file: `src/lib.rs`
- Verify: `cargo test -p openticker-core`

## Current Working Shape

- Small contextual module split with `src/lib.rs` as re-export surface.
- Internal files: `error.rs`, `identifiers.rs`, `market.rs`, `signals.rs`, `timeframe.rs`, `trade.rs`.
- Timeframe parsing and serde behavior are hand-written and important.
- Only a small identifier wrapper set exists today: `InstanceId`, `AccountId`, `BotLaneKey`.

## Invariants

- Keep this crate dependency-light and connector-agnostic.
- Do not move HTTP, runtime, storage, or venue behavior into this crate.
- Preserve serialized enum labels and timeframe strings unless the change is intentional and coordinated.

## Common Change Recipes

### Add a new shared enum or struct

1. Confirm it is truly shared across multiple crates.
2. Prefer a small additive type over changing an existing one if compatibility matters.
3. Add serde and formatting behavior explicitly when needed.
4. Re-run dependent crate tests after the change, not just this crate's tests.

## Watchouts

- Even small changes here can break config parsing, runtime journaling, or HTTP serialization downstream.

## Common Follow-Ups

- Re-test dependent crates when you change any public type, especially `Timeframe`, `IndicatorSignal`, `TradeIntent`, or role-related enums.
