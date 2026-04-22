# Contributing

## Scope

OpenTicker is a Rust workspace with explicit crate boundaries. Prefer the smallest correct change and keep behavior localized to the right crate.

## Before Opening A Pull Request

1. Run the focused crate tests for the code you changed.
2. Run the workspace checks when the change crosses crate boundaries.
3. Update config examples or README content if the operator-facing surface changed.

Useful commands:

```bash
make fmt
make check
make test
make ci
```

## Change Guidelines

1. Keep risk logic pure in `openticker-risk`.
2. Keep indicator logic pure in `openticker-signals`.
3. Keep connector-specific payloads inside `openticker-connectors`.
4. Prefer additive migrations when touching persisted runtime records.
5. Avoid checking secrets or live credentials into the repository.

## Reporting Issues

When filing a bug, include:

1. the crate or surface area involved
2. the config shape or sample input that reproduces the issue
3. expected behavior
4. actual behavior
5. relevant logs or test output
