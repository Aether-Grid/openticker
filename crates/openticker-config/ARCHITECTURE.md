# ARCHITECTURE

Last reviewed: 2026-04-18

## Role

`openticker-config` owns the deployment configuration model for the system.

It is responsible for:

- parsing on-disk TOML config
- loading account, risk, and instance files into one bundle
- resolving secrets from environment variables
- performing semantic validation across files
- producing a safe `effective_config` view for operators and HTTP consumers

The public entrypoint is `load_from_dir`, re-exported from `src/lib.rs` and implemented in `src/loading.rs`.

## Entry Surface

Important public types:

- `GlobalConfig`
- `ServiceConfig`
- `HttpConfig`
- `StorageConfig`
- `ObservabilityConfig`
- `SafetyConfig`
- `DataPlaneConfig`
- `DataPlaneWatchlistEntry`
- `AccountConfig`
- `RiskProfileConfig`
- `InstanceConfig`
- `IndicatorInstanceConfig`
- `RiskOverrides`
- `ConfigBundle`
- `EffectiveConfig`
- `EffectiveAccountConfig`
- `AccountSecretStatus`
- `ConfigError`

Important public functions and methods:

- `load_from_dir(config_dir)`
- `ConfigBundle::validate()`
- `ConfigBundle::effective_config()`
- `AccountConfig::execution_remote_submission_enabled()`

## Internal Layout

The crate is module-split by concern.

| Path | Responsibility |
| --- | --- |
| `src/lib.rs` | Module wiring and public re-exports |
| `src/model.rs` | Schema structs and bundle/effective-config model types |
| `src/loading.rs` | Dotenv loading, directory resolution, and TOML readers |
| `src/validation.rs` | Semantic validation and effective-config projection |
| `src/error.rs` | `ConfigError` definition |
| `src/tests.rs` | Crate-level tests |

## Direct Dependency Wiring

Workspace dependencies:

| Crate | Used For |
| --- | --- |
| `openticker-core` | Shared enums and types such as `ExecutionMode`, `MarketType`, `Timeframe`, `IndicatorRole`, `IndicatorSignalPolicy`, `IndicatorStabilityClass` |
| `openticker-signals` | Indicator manifest lookup during validation |

Important point:

- validation uses `openticker_signals::indicator_manifest(...)`, but runtime instantiation still happens elsewhere in `openticker-runtime`

## Inbound Wiring

Primary consumers:

- `openticker-runtime` loads `ConfigBundle` and builds runtime state from it
- `openticker-http` reloads config and serves effective-config views
- `openticker-cli` uses it for `validate-config`, `print-effective-config`, and service startup bootstrapping

## Outbound Wiring

Outbound dependencies are narrow:

- to `openticker-core` for domain enums and identifiers used in schema
- to `openticker-signals` for manifest-driven indicator validation

This crate does not call into runtime, connectors, or HTTP code.

## Load And Validation Flow

Current loading flow is:

1. load `.env` from config dir, parent, or ambient environment
2. read `openticker.toml`
3. resolve configured subdirectories
4. load account files
5. load risk profile files
6. load instance files from `global.service.bot_dir`
7. validate cross-references, connector assumptions, and indicator declarations
8. return `ConfigBundle`
9. derive `EffectiveConfig` when a safe inspection view is needed

## Current Implementation Realities

- Connector capability knowledge is duplicated locally instead of being sourced from `openticker-connectors`.
- Strategy selection is still effectively string-based. Config can carry a strategy name that runtime rejects later if it is unsupported.
- Indicator validation is manifest-driven, but that manifest is not yet the single source of truth for runtime wiring.
- The active instance directory is driven by `global.service.bot_dir`, even though some older docs refer to `instances/` specifically.
- Validation errors are deliberately operator-readable and should stay that way.

## Practical Wiring Notes

- This crate is the first semantic gate before runtime construction.
- Its `effective_config()` output is intentionally secret-safe and is consumed by operator surfaces.
- Any field added here usually requires coordinated follow-up in `openticker-runtime`, and sometimes in `openticker-http` and `openticker-cli`.

## Diagram

```mermaid
flowchart TD
  Root[config dir]
  Env[.env and ambient env]
  Global[openticker.toml]
  Accounts[accounts/*.toml]
  Risk[risk/*.toml]
  Bots[service.bot_dir/*.toml]
  Bundle[ConfigBundle]
  Validate[validate()]
  Effective[effective_config()]

  Root --> Env
  Root --> Global
  Root --> Accounts
  Root --> Risk
  Root --> Bots
  Env --> Bundle
  Global --> Bundle
  Accounts --> Bundle
  Risk --> Bundle
  Bots --> Bundle
  Bundle --> Validate --> Effective
```
