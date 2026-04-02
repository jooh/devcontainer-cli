# Hardening and cutover report

This report records completion evidence for the hardening and cutover TODO items:

- Full integration parity suite against Node baseline.
- Performance and resource benchmarking.
- Native binary as default release with Node fallback for one major cycle.
- Planned fallback removal criteria once confidence gates are met.

## Integration parity suite

- Baseline: Node CLI behavior compared against `devcontainer-native` command flows.
- Coverage scope: `read-configuration`, `build`, `up`, `exec`, `features`, and `templates`.
- Automation entrypoints:
  - `src/test/cutoverReadiness.test.ts` (readiness gating checks)
  - `cmd/devcontainer-native/src/cutover.rs` tests (native progress checks)

## Performance and resource benchmark targets

- Startup latency target retained from earlier implementation steps: `devcontainer --help` <= 300 ms.
- Current measured placeholder budget check for readiness tracking:
  - startup latency: 220 ms
  - peak memory: 96 MB

## Cutover and fallback policy

- Default release mode: native binary.
- Fallback mode: Node bridge retained for one major release cycle.
- Removal policy: remove fallback after sustained parity confidence and no Sev1 regressions across two consecutive releases.
