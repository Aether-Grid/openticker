# openticker-registry

Build-specific registry and engine construction surface for OpenTicker.

This crate currently aggregates:

- built-in indicator descriptors from `openticker-signals`
- optional extension indicator descriptors from `openticker-indicators` behind the `indicators` feature

It is the build-specific lookup surface consumed by `openticker-config` and `openticker-instance`.
