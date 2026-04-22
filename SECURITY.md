# Security Policy

## Reporting A Vulnerability

Do not open a public GitHub issue for suspected security problems or credential leaks.

Report them privately to the maintainers through the repository security advisory flow or the contact path documented by the project owners.

Please include:

1. a clear description of the issue
2. affected crates, commands, or endpoints
3. reproduction steps if available
4. impact assessment
5. any suggested mitigation

## Secrets And Credentials

This repository is designed to reference credentials through environment variable names such as `api_key_env` and `api_secret_env`. Real secret values must never be committed.

If you believe a secret was committed historically, report it privately so it can be rotated and removed from history.
