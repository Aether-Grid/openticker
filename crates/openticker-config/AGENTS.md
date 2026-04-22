# AGENTS.md

Last reviewed: 2026-04-18

## Overview

This directory contains the config schema and validation layer. Changes here affect deployment shape, startup validity, effective-config inspection, and sometimes runtime construction.

## Package And Commands

- Cargo package: `openticker-config`
- Entrypoint file: `src/lib.rs`
- Verify: `cargo test -p openticker-config`

## Current Working Shape

- Public API is re-exported from `src/lib.rs`.
- Schema and bundle model types live in `src/model.rs`.
- Loading and TOML parsing helpers live in `src/loading.rs`.
- Semantic validation and `effective_config` rendering live in `src/validation.rs`.
- Error types live in `src/error.rs`.
- Crate tests live in `src/tests.rs`.
- `load_from_dir` is the main entry point.
- Bot config files are loaded from `global.service.bot_dir`, which defaults to `bots/`.
- Validation depends on local connector capability tables and `openticker_signals::indicator_manifest`.
- `effective_config` is the safe inspection view used by other crates.

## Invariants

- Preserve the on-disk layout: `openticker.toml`, plus `accounts`, `risk`, and the configured `global.service.bot_dir` subtree.
- Validation errors should stay operator-readable and specific.
- Never expose secret values through effective-config helpers.
- Keep indicator rules aligned with the signal manifest.

## Common Change Recipes

### Add a new config field

1. Add it to the appropriate schema struct.
2. Update TOML parsing expectations if needed.
3. Add validation if the field affects safety or runtime assumptions.
4. Update `config/` examples or bot files if the field is required or behaviorally important.
5. Update downstream runtime wiring if the field changes execution behavior.

### Add a new connector kind

1. Update the local connector capability table in this crate.
2. Update validation for required secrets, roles, and market support.
3. Coordinate matching changes in `openticker-connectors`.

### Add a new indicator type or role rule

1. Update `openticker-signals` first if the manifest needs new metadata.
2. Confirm `validate_indicators` enforces the new capability or role rules correctly.
3. Update config examples if deployable bots should use the new type.

## Watchouts

- Connector knowledge is duplicated here and in the connector crate.
- Runtime support and config support are separate concerns; a type that validates here still needs runtime construction support elsewhere.

## Common Follow-Ups

- Update `crates/openticker-runtime` when schema changes affect instance construction or safety behavior.
- Update `crates/openticker-cli` and `crates/openticker-http` when effective-config output changes.
