# openticker-config

Last reviewed: 2026-04-18

Configuration loading, validation, and effective-config rendering for the OpenTicker workspace.

## Purpose

`openticker-config` is the typed front door into a deployment. It reads the config directory, resolves secrets from the environment, validates cross-file references, and returns a `ConfigBundle` that other crates can trust.

## Current Architecture

The crate is split by concern:

- `src/lib.rs` wires modules and re-exports the public surface
- `src/model.rs` defines schema and effective-config model types
- `src/loading.rs` owns `.env` resolution, directory traversal, and TOML loading
- `src/validation.rs` owns semantic validation and effective-config projection
- `src/error.rs` defines `ConfigError`
- `src/tests.rs` contains crate-level tests

The main layers are still the same:

- schema types for global, account, risk-profile, and bot or instance configuration
- directory and TOML file loading helpers
- environment loading and secret-presence validation
- semantic validation across accounts, connectors, indicators, risk profiles, and instances
- `effective_config` rendering that exposes secret presence without printing secret values

## Config Model

The primary public types are:

- `GlobalConfig`
- `AccountConfig`
- `RiskProfileConfig`
- `InstanceConfig`
- `IndicatorInstanceConfig`
- `ExecutionConstraintsConfig`
- `ConfigBundle`
- `EffectiveConfig`

`InstanceConfig` is the most important runtime-facing record. It binds market, symbols, timeframe, account, connectors, strategy, signal mode, indicators, execution constraints, and risk profile selection into one deployable unit.

## How Loading Works

`load_from_dir` performs the current load pipeline:

1. load `.env` from the config directory, its parent, or the ambient environment
2. read `openticker.toml`
3. read `accounts/*.toml`
4. read `risk/*.toml`
5. read bot config files from `global.service.bot_dir`, which defaults to `bots/*.toml`
6. build a `ConfigBundle`
7. run semantic validation

If validation succeeds, downstream crates receive one coherent bundle rather than doing piecemeal checking themselves.

## What Validation Covers

The current validator checks:

- unique IDs across accounts, risk profiles, instances, and indicators
- storage backend rules, currently only `sqlite`
- connector kind support and market compatibility
- account execution-mode constraints and required secret env vars
- instance account and connector bindings
- indicator type support through `openticker_signals::indicator_manifest`
- role, signal-policy, market-support, and preview or confirmed capability checks for indicators
- execution-constraint numeric sanity
- risk override sanity

## Current State

- Connector capabilities are currently hard-coded inside this crate for `alpaca` and `binance`.
- Indicator support is driven by the signal manifest, but runtime instantiation is still wired separately in `openticker-runtime`.
- `effective_config` intentionally exposes secret presence booleans instead of values so CLI and HTTP inspection remain safe.
- The crate is stricter than a plain schema parser. It rejects deployments that would only fail later in runtime.

## Refactor Notes

- Connector capability metadata is duplicated across the workspace and is a likely future cleanup point.
- Validation logic is now centralized in `src/validation.rs`; splitting validator helpers into submodules is still a likely next step if rule count keeps growing.
- Adding a new config field usually requires changes in schema, validation, examples under `config/`, and often runtime wiring.

## Expected On-Disk Layout

`load_from_dir` expects:

- `openticker.toml`
- `accounts/*.toml`
- `risk/*.toml`
- `${global.service.bot_dir}/*.toml`, with the default being `bots/*.toml`

## Verify

- `cargo test -p openticker-config`
