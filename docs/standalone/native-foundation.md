# Native runtime notes

The native CLI lives in `cmd/devcontainer` and is organized as a library crate with a thin binary entrypoint.

## Key pieces

- `src/lib.rs`: top-level argument flow and unsupported-path handling.
- `src/cli.rs`: help and log-format handling.
- `src/commands/`: thin command-family adapters.
- `src/runtime/`: runtime subsystems for build/container/context/exec/lifecycle behavior.
- `src/config.rs`: JSONC and config resolution helpers.
- `src/process_runner.rs`: subprocess execution.

## Current command shape

- `read-configuration`, `build`, `up`, `set-up`, `run-user-commands`, `outdated`, `upgrade`, and `exec` are handled natively.
- `features` and `templates` are native for local flows tracked by the crate today.
- Node is no longer used as a runtime bridge for unsupported command paths.

See `docs/architecture.md` for the current contributor-facing layout.
