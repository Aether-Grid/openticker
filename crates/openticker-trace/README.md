# openticker-trace

Last reviewed: 2026-04-19

Typed cycle inspection DTOs and pure trace-model helpers for OpenTicker.

## Purpose

`openticker-trace` is the read-model crate for cycle inspection. It defines the
stable payloads used to explain one evaluated lane decision across trigger,
signal, intent, risk, execution, position, capital, and reconciliation.

The crate is intentionally transport-agnostic:

- no HTTP handlers
- no static assets
- no runtime mutation
- no storage backends

## Verify

- `cargo test -p openticker-trace`
