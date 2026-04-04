# Native foundation report (Rust) (cutover runtime)

Status updated: 2026-04-03

## Rust crate foundation
- Added an in-repo Rust crate at `cmd/devcontainer`.
- Crate defines an initial native binary target named `devcontainer` to host incremental command ports.

## Top-level CLI parity scaffold
- Added a native foundation readiness evaluator that verifies parity coverage for required top-level command surfaces:
  - `read-configuration`
  - `build`
  - `up`
  - `exec`
  - `features`
  - `templates`
- Evaluator includes help text parity gating (`helpParity`) so parity checks require both command presence and help alignment.

## Logging and exit-code parity checks
- Added explicit native foundation readiness gating for logging output formats:
  - `text`
  - `json`
- Evaluator also requires exit code parity verification to pass (`exitCodeParity`).

## Runtime cutover checks
- The Rust runtime no longer invokes Node for command execution.
- `DEVCONTAINER_NATIVE_ONLY=1` remains as a regression guard for unsupported paths and local validation.
- Native subcommand help is available for the full tracked top-level command surface.

## Test coverage
- Added native foundation readiness unit tests covering:
  - successful completion when all checks pass
  - failure mode when compatibility bridge requirements are not met
- Added a startup contract check that builds the Rust binary and verifies native help plus implemented commands with `PATH` excluding Node.
- Added a no-node-runtime guard that fails if runtime bridge references are reintroduced into `cmd/devcontainer` or the standalone build script.
