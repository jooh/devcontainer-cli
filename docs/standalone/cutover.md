# Hardening and cutover report

This report tracks cutover progress against the hardening and cutover TODO items. The repository is not yet at full cutover:

- Integration parity coverage is still being expanded command-by-command.
- The Node compatibility bridge still exists for unported command paths.
- Native-only mode is now enforced as a guardrail for CI and local validation.

## Integration parity suite

- Baseline: Node CLI behavior compared against `devcontainer` command flows.
- Coverage scope target: `read-configuration`, `build`, `up`, `exec`, `features`, and `templates`.
- Automation entrypoints:
  - `src/test/cutoverReadiness.test.ts` (readiness gating checks)
  - `cmd/devcontainer/src/cutover.rs` tests (native progress checks)
  - `build/check-native-only.js` (native startup contract without Node on `PATH`)

## Performance and resource benchmark targets

- Startup latency target retained from earlier implementation steps: `devcontainer --help` <= 300 ms.
- Current measured placeholder budget check for readiness tracking:
  - startup latency: 220 ms
  - peak memory: 96 MB

## Cutover and fallback policy

- Default release mode target: native binary.
- Current fallback mode: Node bridge retained until core and collection command parity is complete.
- Removal policy target: remove fallback after sustained parity confidence and no Sev1 regressions across two consecutive releases.

## Upstream submodule cutover migration note

- Upstream submodule cutover is complete: `upstream/` is the canonical source of upstream TypeScript CLI code for compatibility validation.
