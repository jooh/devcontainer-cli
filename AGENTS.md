# AGENTS.md

## Scope
These instructions apply to the entire repository tree rooted at this directory.

## Development approach
- Use red/green TDD
- Commit your changes each time you complete a step in a list
- If on `main`, checkout a feature branch before making commits. Keep using the existing branch if not on `main`
- Push to origin before reporting back at the end of the turn
- Do not create PRs unless asked. PRs should be opened in published, not draft form

## Upstream submodule policy
- The `upstream/` directory is the canonical location for upstream `devcontainers/cli` TypeScript sources.
- The `spec/` directory is the canonical location for upstream `devcontainers/spec` schemas and specification docs.
- Do **not** introduce new copies of upstream- or spec-owned files at repository root.
- Keep project-owned implementation and migration work outside `upstream/` and `spec/` unless explicitly updating submodule pointers.

## Compatibility baseline
- Treat the pinned `upstream/` submodule commit as the compatibility target.
- When changing compatibility-sensitive behavior, prefer tests/logging that make the current upstream commit easy to identify.

## Updating upstream
When asked to update upstream:
1. Update the submodule pointer in `upstream/`.
2. Run/adjust parity tests and related fixtures against the new upstream revision.
3. Keep changes reviewable (submodule bump + project-owned compatibility fixes).

## Pathing expectations
- Tests, scripts, and docs that need upstream assets should reference paths under `upstream/...` explicitly.
- Tests, scripts, and docs that need spec assets should reference paths under `spec/...` explicitly.
- Avoid hardcoded assumptions that upstream files exist at repository root.

## Submodule bump checklist
- Use `git submodule update --init --recursive` before running migration/parity checks.
- Record the new pinned revision with `git rev-parse HEAD:upstream` in PR notes/tests when changing compatibility behavior.
- Record the pinned spec revision with `git rev-parse HEAD:spec` when changing schema-sensitive behavior.
- Keep submodule updates reviewable by separating the submodule pointer bump from project-owned compatibility fixes when practical.

## Spec references to consult for build/config work
- Primary schema baseline: `spec/schemas/devContainer.base.schema.json`.
- Aggregated schema view: `spec/schemas/devContainer.schema.json`.
- Feature schema: `spec/schemas/devContainerFeature.schema.json`.
- Normative config behavior reference: `spec/docs/specs/devcontainerjson-reference.md`.
