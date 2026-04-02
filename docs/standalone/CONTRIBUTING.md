# Standalone / Migration Contributor Guardrails

This folder documents how to keep setup and migration scaffolding isolated from core implementation code.

## Original implementation

Treat the following as **original implementation**:

- Runtime and feature logic under `src/spec-node/` (except `src/spec-node/migration/`).
- Shared specification behavior in `src/spec-common/`, `src/spec-utils/`, `src/spec-configuration/`, and `src/spec-shutdown/`.
- Existing production CLI flows and command wiring.

Changes in these areas should be reviewed as production refactors and can impact baseline behavior for downstream forks.

## Setup / migration scaffolding

Treat the following as **setup/migration scaffolding**:

- Files under `src/spec-node/migration/`.
- Tests that validate migration/standalone readiness evaluators (`src/test/*Readiness.test.ts`).
- Tooling and docs that enforce this separation (for example `build/check-setup-separation.js` and this document).

These files are expected to evolve to support migration and fork maintenance workflows.

## Where refactors are safe

- Prefer placing migration/setup-only logic in `src/spec-node/migration/`.
- Refactors within `src/spec-node/migration/` are generally safe when they do not change production runtime behavior outside migration flows.
- Do **not** add new setup-only files directly under `src/spec-node/`; use the migration namespace instead.
- If setup behavior needs production integration, keep integration points minimal and document the boundary in PR notes.

## Enforcement

Run:

```bash
npm run check-setup-separation
```

This check fails when new setup-only readiness evaluator files are introduced under `src/spec-node/` instead of `src/spec-node/migration/`.
