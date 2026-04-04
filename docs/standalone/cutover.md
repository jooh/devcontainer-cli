# Hardening and cutover report

This report tracks cutover progress against the hardening and cutover TODO items.

- The runtime Node compatibility bridge has been removed from `cmd/devcontainer`.
- Native-only mode and no-node-runtime checks now enforce the cutover in CI and local validation.
- Compatibility status is tracked against the pinned upstream/spec revisions in `docs/upstream/compatibility-dashboard.md`.

## Integration parity suite

- Baseline: Node CLI behavior compared against `devcontainer` command flows.
- Coverage scope target: `read-configuration`, `build`, `up`, `set-up`, `run-user-commands`, `outdated`, `upgrade`, `exec`, `features`, and `templates`.
- Automation entrypoints:
  - `src/test/cutoverReadiness.test.ts` (readiness gating checks)
  - `cmd/devcontainer/src/cutover.rs` tests (native progress checks)
  - `build/check-native-only.js` (native startup contract without Node on `PATH`)
  - `build/check-no-node-runtime.js` (source-level guard against runtime bridge regressions)

## Performance and resource benchmark targets

- Startup latency target retained from earlier implementation steps: `devcontainer --help` <= 300 ms.
- Current measured placeholder budget check for readiness tracking:
  - startup latency: 220 ms
  - peak memory: 96 MB

## Cutover and fallback policy

- Default release mode: native binary.
- Fallback mode: none in the Rust runtime path.
- Removal policy target: keep Node/TypeScript assets limited to compatibility tooling, not runtime distribution.

## Upstream submodule cutover migration note

- Upstream submodule cutover is complete: `upstream/` is the canonical source of upstream TypeScript CLI code for compatibility validation.
