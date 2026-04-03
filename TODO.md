# TODO: Native Rust Cutover Plan (No Node Runtime)

## Objective
Ship `devcontainer` as a **standalone Rust binary** that does not require Node.js, JavaScript bundles, or TypeScript artifacts at runtime.

---

## Current-state analysis (what exists today)

### Native binary status (project-owned Rust)
- `cmd/devcontainer` is currently a **partial shim**, not a full port.
- Native behavior exists only for:
  - `read-configuration` with a limited option surface (`--workspace-folder`, `--config` only).
  - `features list|ls` and `templates list|ls` returning placeholder empty arrays.
- Most commands still delegate to `node dist/spec-node/devContainersSpecCLI.js` via compatibility bridge.
- If Node or the JS bundle is unavailable, commands fail at runtime (matches reported issue).

### Repository messaging mismatch to resolve
- Several readiness docs/tests currently mark native/cutover phases as “completed”, but actual Rust implementation still depends on Node for major behavior.
- Immediate follow-up should align docs/readiness gates with implementation reality, or raise implementation to match documented completion.

### Packaging/runtime state
- npm package still points CLI bin entry to Node entrypoint under `upstream/devcontainer.js`.
- Root project still includes JS/TS build + test pipeline and Node dependency graph.

---

## Upstream reference analysis (`upstream/` TypeScript baseline)

### Scope snapshot
- Upstream has a broad implementation surface (109 TypeScript files under `upstream/src`).
- Core command/CLI layer lives in `upstream/src/spec-node/devContainersSpecCLI.ts`.
- Module families to port:
  - `spec-node` (command handlers, Docker/build/up/exec orchestration, features/templates CLIs).
  - `spec-common` (CLI host, process/PTY/native module abstraction, env injection).
  - `spec-configuration` (devcontainer config model parsing/validation/substitution).
  - `spec-utils` (logging/events/workspace/product helpers).
  - `spec-shutdown` (Docker process/pty integration).

### Command surface parity target (from upstream CLI)
Top-level commands in scope for full native parity:
- `up`
- `set-up`
- `build`
- `run-user-commands`
- `read-configuration`
- `outdated`
- `upgrade`
- `features` subcommands (`test`, `package`, `publish`, `info`, `resolve-dependencies`, `generate-docs`)
- `templates` subcommands (`apply`, `publish`, `metadata`, `generate-docs`)
- `exec`

## Specification reference analysis (`spec/` baseline)

### Scope snapshot
- `spec/` is a pinned submodule reference for normative schema/docs.
- Primary machine-readable schema baseline is `spec/schemas/devContainer.base.schema.json`.
- Aggregated schema (`spec/schemas/devContainer.schema.json`) layers in editor/tool overlays and is still useful for compatibility visibility.
- Behavioral semantics that are not fully captured by schema shape live in `spec/docs/specs/devcontainerjson-reference.md` and related docs.

### Planning impact
- We should explicitly treat schema compatibility as a first-class parity lane alongside upstream CLI behavior parity.
- We can and should consume schemas directly from `spec/schemas/...` for validation tests/fixtures rather than maintaining duplicated local schema copies.

---

## Target architecture (Rust-only runtime)

### Principles
- No runtime dependency on Node, npm, or bundled JS.
- Keep compatibility target pinned to `HEAD:upstream` submodule commit.
- Preserve user-facing compatibility for:
  - exit codes,
  - machine-readable JSON outputs,
  - major text/diagnostic messages,
  - Docker/Compose behavior.

### Proposed crate/module layout (inside `cmd/devcontainer`)
- `cli/`: argument parsing + command dispatch.
- `config/`: JSONC parsing, schema-ish validation, variable substitution, merge logic.
- `docker/`: Docker/Compose command construction + execution wrappers.
- `lifecycle/`: lifecycle hook orchestration (`onCreate`, `postCreate`, etc.).
- `features/`: feature resolution, OCI fetch/metadata/pack/publish flow.
- `templates/`: template metadata/apply/publish/docs flow.
- `exec/`: command execution, terminal behavior, env propagation.
- `output/`: structured output contracts + formatting parity.
- `compat/`: upstream parity fixtures and translators (temporary during migration).

---

## Execution plan

## Phase 0 — Reset and truth alignment
- [ ] Mark the current Node-bridge state as transitional in docs/readiness checks.
- [ ] Add explicit CI check that fails when any command path shells out to Node in “native-only” mode.
- [ ] Add startup/runtime contract test: running binary with `PATH` excluding Node still supports implemented commands.

## Phase 1 — Build parity harness against upstream
- [ ] Auto-generate command/option matrix from upstream yargs definitions for drift detection.
- [ ] Ensure parity matrix includes full upstream top-level scope: `up`, `set-up`, `build`, `run-user-commands`, `read-configuration`, `outdated`, `upgrade`, `features`, `templates`, `exec`.
- [ ] Build golden test corpus from upstream fixtures for:
  - `read-configuration`
  - config substitution/merge behavior
  - Docker command construction
  - JSON output schemas
- [ ] Build schema contract tests against pinned `spec/` submodule:
  - [ ] Validate accepted configs against `spec/schemas/devContainer.base.schema.json`.
  - [ ] Validate known-invalid configs fail with expected error categories/messages.
  - [ ] Track schema revision provenance in test output (`git rev-parse HEAD:spec`).
- [ ] Add dual-run test harness: execute same scenario against upstream CLI and Rust CLI, diff outputs/exit codes.
- [ ] Treat text/log/output parity as **semantic equivalence** unless a contract explicitly requires byte-level matching.
- [ ] Add a schema drift check that fails when pinned `HEAD:spec` changes without corresponding schema-parity fixture/test updates.

## Phase 2 — Port foundational libraries (non-command-specific)
- [ ] Implement Rust logging/event primitives compatible with upstream formats (`text`/`json`).
- [ ] Implement config discovery + JSONC parsing + substitution semantics.
- [ ] Implement CLI host/environment probing abstractions currently in `spec-common`.
- [ ] Implement subprocess wrappers with controlled stdio capture and streaming.

## Phase 3 — Port core commands first
- [ ] `read-configuration` full parity (all flags and metadata inclusion options).
- [ ] `build` parity (including BuildKit toggles, cache options, labels, Dockerfile/Compose paths).
- [ ] `up` parity (container lifecycle + post-commands + mounts/env handling).
- [ ] `set-up`, `run-user-commands`, and `outdated` parity.
- [ ] `exec` parity with staged gates:
  - [ ] GA gate: non-interactive CI behavior parity.
  - [ ] post-GA hardening gate: interactive TTY/PTY fidelity.

## Phase 4 — Port collection and publishing flows
- [ ] `features`: `resolve-dependencies`, `info`, `test`, `package`, `publish`, `generate-docs`.
- [ ] `templates`: `apply`, `metadata`, `publish`, `generate-docs`.
- [ ] `upgrade` parity and lockfile behavior.
- [ ] Sequence network-dependent `features/templates publish` parity after local/resolve/test/apply parity is stable.

## Phase 5 — Hardening + cutover
- [ ] Remove Node bridge codepath entirely from `devcontainer`.
- [ ] Remove runtime assumptions about `dist/spec-node/devContainersSpecCLI.js`.
- [ ] Add release CI lanes for multi-platform Rust binaries.
- [ ] Publish release artifacts via **GitHub Releases** (no npm publication for current rollout).
- [ ] Introduce compatibility dashboard keyed to pinned `upstream` commit.
- [ ] Switch default distributed executable to native binary.
- [ ] Update cutover docs continuously as implementation status changes (no “completed” claims ahead of verified parity).

## Phase 6 — Repository cleanup after successful cutover
- [ ] Move TS/Node build/test assets to compatibility-only role (or separate tooling repo) if no longer needed for distribution.
- [ ] Keep npm/bin metadata out of the native release path while GitHub Releases remains the distribution channel.
- [ ] Keep `upstream/` as reference baseline and parity fixture source.

---

## Definition of done (native milestone)
- [ ] `devcontainer` runs full target command set on machines without Node installed.
- [ ] No runtime subprocess invocation of `node` for any GA command path.
- [ ] Output/exit-code parity suite passes against pinned upstream baseline.
- [ ] Schema contract suite passes against pinned `spec` baseline.
- [ ] Published artifact is a standalone Rust binary for target platforms.
