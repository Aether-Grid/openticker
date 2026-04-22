# openticker-gateway

Last reviewed: 2026-04-18

Runtime-agnostic facade over connector-registry operations used by the runtime.

It owns readiness-gated connector access plus normalization of connector symbol
constraints into the shared execution-constraint shape, and connector-registry
construction from validated account config.
