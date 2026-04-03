# AGENTS.md

## Scope
These instructions apply to the entire repository tree rooted at this directory.

## Upstream submodule policy
- The `upstream/` directory is the canonical location for upstream `devcontainers/cli` TypeScript sources.
- Do **not** introduce new copies of upstream-owned files at repository root.
- Keep project-owned implementation and migration work outside `upstream/` unless explicitly updating the submodule pointer.

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
- Avoid hardcoded assumptions that upstream files exist at repository root.

## Submodule bump checklist
- Use `git submodule update --init --recursive` before running migration/parity checks.
- Record the new pinned revision with `git rev-parse HEAD:upstream` in PR notes/tests when changing compatibility behavior.
- Keep submodule updates reviewable by separating the submodule pointer bump from project-owned compatibility fixes when practical.
