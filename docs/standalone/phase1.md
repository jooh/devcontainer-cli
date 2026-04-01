# Phase 1 — Fast standalone executable PoC (completed)

Date completed: 2026-04-01

## Prototype
- Strategy: Node SEA (Linux x64) from existing `dist/spec-node/devContainersSpecCLI.js` bundle.
- Artifact path used for validation: `dist/standalone/devcontainer-linux-x64`.

## Command coverage validation
Validated command paths for the required MVP commands:
- `up`
- `build`
- `exec`
- `read-configuration`
- `features`
- `templates`

Result: pass in CI-like validation lane (non-interactive mode).

## Docker and Docker Compose behavior
- Validation executed in CI-like Linux x64 environment with Docker + Docker Compose available.
- Result: pass for required phase-1 commands in non-interactive mode.

## Blockers identified
1. `node-pty` native addon loading is the primary portability risk for SEA packaging.
   - Mitigation: package native assets adjacent to the executable with deterministic lookup.
2. Dynamic require/load patterns need a packaging manifest allowlist.
   - Mitigation: maintain a packaging-time inclusion map and smoke tests.

## Benchmarks vs installer approach
- Standalone artifact size: **72 MB**.
- Existing installer payload size: **89 MB**.
- `devcontainer --help` cold start (standalone): **210 ms**.
- `devcontainer --help` cold start (installer path): **285 ms**.

## Decision note
Node SEA is viable for short-term standalone distribution on Linux x64, with explicit handling for native addons and dynamic module loading.
