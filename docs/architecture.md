# Native CLI Architecture

## Runtime layout

- `cmd/devcontainer/src/lib.rs`: top-level runtime entrypoint and shared process-wide guards.
- `cmd/devcontainer/src/cli.rs`: help text, log-format parsing, and command-surface metadata.
- `cmd/devcontainer/src/commands/`: native command handlers grouped by behavior.
- `cmd/devcontainer/src/config.rs`: config-path resolution, JSONC parsing, and local variable substitution.
- `cmd/devcontainer/src/process_runner.rs`: subprocess helpers for captured and streaming execution.
- `cmd/devcontainer/src/output.rs`: text/json log rendering helpers.

The binary entrypoint in `cmd/devcontainer/src/main.rs` is intentionally thin and just calls into the library crate.

## Command modules

- `commands/configuration.rs`: `read-configuration`, `build`, lifecycle-style commands, `outdated`, and `upgrade`.
- `commands/exec.rs`: native `exec` handling, including interactive vs captured execution.
- `commands/collections.rs`: `features` and `templates` command families.
- `commands/common.rs`: shared CLI option parsing, config loading, manifest helpers, and packaging/file-copy helpers.

## Test layers

- Rust unit tests live next to the implementation modules.
- Rust integration tests live under `cmd/devcontainer/tests/`.
- Repo-owned compatibility fixtures live under `src/test/parity/`.
- Node guard scripts in `build/` cover upstream/spec drift, command-matrix drift, native-only startup, no-node-runtime regressions, and the current parity harness.

## Compatibility assets

- `upstream/` is the only canonical location for upstream CLI TypeScript sources.
- `spec/` is the only canonical location for upstream schemas and normative spec docs.
- Root-level Node scripts are compatibility tooling only; they must not become part of runtime execution or release packaging.

## Maintenance rules

- Keep new runtime logic in the Rust crate, not in root-level Node/TypeScript code.
- Prefer behavior-level command modules over growing `lib.rs` or `main.rs`.
- When a compatibility check needs new fixtures, add them under `src/test/parity/` and keep them tied to the pinned submodule revisions.
