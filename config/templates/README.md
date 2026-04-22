# Config Templates

This directory mirrors the main `config/` layout with commented starter templates.

Available templates:

- `openticker.toml`
- `accounts/alpaca-paper.toml`
- `accounts/alpaca-live.toml`
- `accounts/binance-demo.toml`
- `accounts/binance-live.toml`
- `risk/equities-default.toml`
- `risk/crypto-default.toml`
- `bots/single-indicator.toml`
- `bots/consensus.toml`
- `bots/momentum-ultima-plus.toml`
- `bots/strong-only.toml`
- `bots/live-single-indicator.toml`

The templates include:

- all current schema fields that can be configured in TOML
- default values where the schema defines them
- commented optional fields and operator notes
- metadata filter examples for entry and exit gating
- multi-symbol basket defaults so lane fan-out behavior is visible out of the box

Indicator-specific parameter support is still runtime-driven.

Current runtime-recognized indicator params:

- `sensitivity`
- `rsi_period`
- `slow_factor`
- `qqe`

Other built-in indicators currently run with their Rust defaults unless runtime wiring is extended.
