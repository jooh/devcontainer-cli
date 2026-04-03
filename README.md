# Devcontainer CLI Native Port

This repository hosts a **project-owned native migration** of the Dev Containers CLI, with compatibility targeted against the upstream TypeScript implementation stored in the `upstream/` git submodule.

## Repository layout and upstream compatibility

`upstream/` exists so we can track the canonical upstream sources at an exact pinned commit while keeping native-port and migration work reviewable in this repository.

- `upstream/`: canonical upstream devcontainers/cli TypeScript baseline.
- repository root: project-owned native implementation, migration checks, docs, and readiness tests.

Compatibility contract: this repository targets the **exact** submodule revision pinned at `HEAD:upstream`.

## Submodule initialization and recovery

If you clone this repository without submodules, initialize them before running checks/builds:

```bash
git submodule update --init --recursive
```

If tooling reports that `upstream/` is missing/uninitialized, run the same command again and re-run checks.

## Upstream compatibility workflow

When updating upstream, use an explicit bump-and-verify flow:

```bash
git submodule update --init --recursive
git -C upstream fetch origin
git -C upstream checkout <new-upstream-commit>
git add upstream
git rev-parse HEAD:upstream
npm run check-upstream-submodule
npm run check-upstream-compatibility
npm test
```

If the compatibility baseline check reports a commit delta, update:

- `docs/upstream/compatibility-baseline.json`

## Local development

Install dependencies and run project tests:

```bash
npm install
npm test
```

Run focused migration/readiness checks:

```bash
npm test -- --grep "upstream submodule cutover"
```

## Project status

Current work focuses on:

- native CLI migration and parity tracking,
- upstream submodule cutover guardrails,
- compatibility baseline visibility and CI checks.

See `TODO.md` for phased migration/cutover tracking.
