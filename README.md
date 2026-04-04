# devcontainer-rs

This repository hosts a **project-owned native migration** of the Dev Containers CLI, with compatibility targeted against the upstream TypeScript implementation stored in the `upstream/` git submodule.

The distributed CLI runtime is now the Rust binary in `cmd/devcontainer`; Node/TypeScript assets remain in the repository for parity tracking against `upstream/`, not for runtime execution.

## Repository layout and upstream compatibility

`upstream/` exists so we can track the canonical upstream sources at an exact pinned commit while keeping native-port and migration work reviewable in this repository.

- `upstream/`: canonical upstream devcontainers/cli TypeScript baseline.
- `spec/`: canonical upstream devcontainers/spec reference (schemas + normative docs).
- repository root: project-owned native implementation, migration checks, docs, and readiness tests.

Compatibility contract: this repository targets the **exact** submodule revision pinned at `HEAD:upstream`.
Specification contract: schema and behavioral validation should reference the **exact** submodule revision pinned at `HEAD:spec`.

## Submodule initialization and recovery

If you clone this repository without submodules, initialize them before running checks/builds:

```bash
git submodule update --init --recursive
```

If tooling reports that `upstream/` or `spec/` is missing/uninitialized, run the same command again and re-run checks.

## Spec reference workflow (`spec/`)

When implementing or validating `read-configuration`/config semantics:

- Use `spec/schemas/devContainer.base.schema.json` as the primary schema baseline.
- Use `spec/schemas/devContainer.schema.json` for the aggregate schema view (including editor-specific overlays).
- Use normative docs in `spec/docs/specs/` (especially `devcontainerjson-reference.md`) for behavioral interpretation not fully captured by schema shape.

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
cargo test --manifest-path cmd/devcontainer/Cargo.toml
```

Run focused migration/readiness checks:

```bash
npm test -- --grep "upstream submodule cutover"
```

## Project status

Current work focuses on:

- native CLI parity tracking against pinned upstream/spec baselines,
- standalone Rust binary release and smoke coverage,
- compatibility guardrails that prevent reintroducing a runtime Node bridge.

See `TODO.md` for phased migration/cutover tracking.
