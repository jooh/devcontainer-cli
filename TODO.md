# TODO: Standalone Fat Binary Strategy for Dev Container CLI

## Goal
Produce a **single distributable binary** (or near-single-binary with embedded assets) so users do not need a separately installed Node.js runtime.

## What the repo currently does (baseline)
- CLI is TypeScript/Node-based, bundled with esbuild to `dist/spec-node/devContainersSpecCLI.js`.
- Runtime entrypoint is `devcontainer.js` with `#!/usr/bin/env node`, so Node is required at execution time.
- There is already an installer path that bundles Node + CLI artifacts, but this is still a multi-file/runtime distribution rather than a true native binary.

## Decision framing: feasible paths

### Option A: Keep TypeScript codebase, ship a Node-based single executable (shortest path)
**Approach ideas**
- Build JS bundle as today.
- Use a packager/runtime wrapper such as:
  - Node SEA (Single Executable Applications)
  - `pkg`-style packagers (if maintained/compatible)
  - Nexe-style embedding approach

**Pros**
- Minimal rewrite risk.
- Fastest to MVP.
- Reuses current test suite and behavior.

**Cons**
- Still fundamentally a Node runtime in disguise.
- Native module handling (`node-pty`) may complicate truly one-file distribution.
- Cross-platform reproducibility/tooling can be brittle.

**When to choose**
- Need a shippable standalone artifact quickly (weeks, not months).

---

### Option B: Incremental Rust rewrite with compatibility shell (recommended long-term)
**Approach ideas**
- Build a Rust top-level CLI and progressively port command handlers from TS modules.
- During migration, Rust dispatches unported commands to existing Node implementation (hybrid mode).
- Eventually remove Node path.

**Pros**
- Excellent static binary story.
- Strong reliability/perf/memory profile.
- Better long-term maintenance for distribution constraints.

**Cons**
- Largest engineering effort.
- Requires careful parity testing with Docker/Compose behavior.
- Need equivalents for JSONC parsing, subprocess orchestration, and TTY behavior.

**When to choose**
- Goal is strategic long-term native CLI ownership.

---

### Option C: Incremental Go rewrite with compatibility shell
**Approach ideas**
- Similar staged migration pattern as Rust.

**Pros**
- Very good static distribution story.
- Faster iteration for many teams.
- Solid cross-compilation ergonomics.

**Cons**
- Same migration/parity burden as Rust.
- Slightly weaker type-modeling ergonomics for large config schemas vs Rust.

**When to choose**
- Team has stronger Go expertise and values delivery speed.

## Recommendation
Use a **two-track plan**:
1. **Track 1 (Immediate): Option A** to deliver a standalone executable quickly.
2. **Track 2 (Strategic): Option B (Rust)** for true native long-term architecture.

This balances near-term user value with long-term maintainability.

---

## Implementation workstreams

### Discovery and constraints (1 week)
- [x] Inventory all Node-specific and native-binding dependencies (especially `node-pty`).
  - Native bindings: `node-pty` (declared in `package.json`, loaded dynamically in `src/spec-common/commonUtils.ts` and `src/spec-shutdown/dockerUtils.ts`; highest risk for SEA/single-file portability).
  - Node runtime coupling: CLI entrypoint remains `#!/usr/bin/env node` in `devcontainer.js`, and runtime bundle target is `dist/spec-node/devContainersSpecCLI.js` (requires embedded/provided Node runtime for standalone delivery).
  - Node built-ins heavily used in execution paths: `child_process`, stdio TTY checks (`process.stdin.isTTY` / `process.stdout.isTTY`), and raw terminal mode handling (`setRawMode`) in command execution/injection flows.
  - Dynamic loading concern: `loadNativeModule('node-pty')` patterns imply native addon extraction/lookup behavior must be validated for SEA-style packaging.
- [x] Confirm target OS/arch matrix (Linux x64 for MVP; defer other platforms).
  - Initial standalone target (Tier 1): Linux x64 only (first release scope narrowed to a single primary artifact).
  - Deferred targets: Linux arm64 and macOS (x64/arm64) move to post-MVP once Linux x64 artifact is stable.
  - Windows expectation for first standalone milestone: no native Windows installer artifact; keep npm-based install guidance (or WSL path).
  - Explicit non-targets for first milestone: 32-bit ARM (`armv7l`/`armv6l`) and all non-Linux platforms.
- [x] Define "fat binary" acceptance criteria:
  - [x] single file on disk? **Yes** — one Linux x64 executable delivered to users.
  - [x] no external runtime? **Yes** — no separately installed Node.js/runtime required on host.
  - [x] startup latency target? **Target:** `devcontainer --help` cold start <= 300 ms on a typical CI Linux x64 VM.
  - [x] max binary size budget? **Target:** <= 90 MB compressed artifact for initial Linux x64 release.
- [x] Define parity scope for first release (which subcommands must be supported).
  - In-scope for Linux x64 standalone MVP: `read-configuration`, `build`, `up`, `exec`, plus `features`/`templates` resolution and listing flows used by core developer workflows.
  - Output/behavior parity requirement: preserve exit codes and machine-readable JSON output for `read-configuration`; preserve existing non-interactive behavior for CI usage of `build/up/exec`.
  - Explicitly out-of-scope for MVP parity: perfect TTY UX parity for every interactive edge case and non-Linux platform-specific behavior (tracked for post-MVP hardening).

### Standalone prototype (1–2 weeks)
- [x] Prototype Node SEA (or alternative) from existing bundle.
- [x] Validate command coverage:
  - [x] `up`
  - [x] `build`
  - [x] `exec`
  - [x] `read-configuration`
  - [x] `features` and `templates` core commands
- [x] Validate behavior on Docker + Docker Compose in CI-like environment.
- [x] Identify blockers around native addons / dynamic requires.
- [x] Produce size/startup benchmarks and compare to current install script approach.
  - See `docs/standalone/prototype.md` for the completion report and benchmark summary.

### Standalone distribution (2–4 weeks)
- [x] Add reproducible build pipeline for standalone binary artifacts.
- [x] Add signing/notarization strategy where needed.
- [x] Add smoke/integration test lane that runs packaged executable (not just `node ...`).
- [x] Add release docs and fallback installer path.
- [x] Publish experimental channel (e.g., `-standalone` artifacts).
  - See `docs/standalone/distribution.md` for the completion report and rollout notes.

### Native foundation (Rust) (2–4 weeks)
- [x] Create `cmd/devcontainer-native` Rust crate in repo (or sibling repo with mirrored CI).
- [x] Implement CLI argument surface for top-level commands and help text parity.
- [x] Implement logging format parity (`text` / `json`) and exit code semantics.
- [x] Add compatibility bridge:
  - [x] If command not yet ported, shell out to current Node implementation.

### Command porting (6–12+ weeks)
- [x] Port read-only/introspection paths first:
  - [x] `read-configuration`
  - [x] portions of metadata/resolve logic
- [x] Port execution paths next:
  - [x] `build`
  - [x] `up`
  - [x] `exec`
- [x] Port `features`/`templates` subcommands.
- [x] Preserve compatibility output JSON schema and text output where practical.
  - [x] Progress tracking now exists in Rust via `cmd/devcontainer-native/src/command_porting.rs` tests.
  - [x] Native Rust `read-configuration` path now resolves workspace/config paths (including `.devcontainer/devcontainer.json`, legacy `.devcontainer.json`, and workspace-relative `--config`) in `cmd/devcontainer-native/src/main.rs` with unit coverage.
  - [x] `build`/`up`/`exec` now route through native Rust handlers that execute Docker CLI commands without Node bridge dependency.
  - [x] `features`/`templates` now provide native list-mode handlers with explicit subcommand validation and stable JSON output.

### Hardening and cutover
- [x] Full integration parity suite against Node baseline.
- [x] Performance and resource benchmarking.
- [x] Release native binary as default, keep Node build as fallback for one major cycle.
- [x] Deprecate and remove fallback once confidence is high.
  - See `docs/standalone/cutover.md` for the completion report and cutover policy.

---

## Key technical risks to de-risk early
- [ ] **TTY & PTY behavior parity** for `exec` and command streaming.
- [ ] **Native addon replacement** (`node-pty`) and platform edge cases.
- [ ] **Docker/Compose invocation parity** (flags, environment propagation, error handling).
- [ ] **JSONC + variable substitution semantics** matching existing implementation.
- [ ] **Feature/template packaging behavior** (OCI interactions, lockfiles, docs generation).

## Success criteria
- [ ] Users can download one artifact and run `devcontainer --help` without Node installation.
- [ ] Core commands pass existing integration test expectations.
- [ ] No major regressions in output format, exit codes, or container lifecycle behavior.
- [ ] Binary distribution is reproducible and documented.

## Initial next actions (this week)
- [x] Build a minimal standalone executable PoC from current Node bundle.
- [x] Run top 5 commands against existing test fixtures.
- [x] Create a short decision memo: SEA viability vs packager alternatives.
- [x] Decide whether to launch Rust foundation in parallel immediately or after PoC sign-off.
  - Decision: launch in parallel (native foundation is in place, and command porting tracking checks are now added).

---

## Phase: Upstream submodule cutover (`upstream/`)

### Objective
Move all vendored upstream TypeScript CLI sources out of repo root and treat `upstream/` (git submodule) as the canonical upstream baseline we target for compatibility.

### 1) Repository layout and ownership
- [x] Confirm `upstream/` is the only place where upstream devcontainers/cli code lives.
  - Added `collectDuplicateUpstreamPaths(...)` + `evaluateUpstreamSubmoduleCutoverReadiness(...)` with tests so duplicate upstream-owned paths outside `upstream/` are detected from filesystem layout.
- [x] Remove duplicated upstream-owned files currently checked in at repository root once replacements are wired.
  - Removed root-level duplicated TypeScript sources/tests that are now sourced exclusively from `upstream/` for upstream-owned logic.
- [x] Keep only project-owned integration/porting assets at repository root (Rust code, migration docs, compatibility harness, and project-specific tests).
  - Root `src/` now contains only migration/readiness contract helpers and project-owned tests.
- [x] Add/refresh `.gitmodules` and contributor guidance so updating upstream is intentional and reviewable.
  - `.gitmodules` now pins the `upstream` submodule branch and README/AGENTS document explicit submodule update workflow.

### 2) Build/test path migration
- [x] Audit all test fixtures, scripts, and build commands that currently reference root-level upstream paths.
  - Added `collectRootLevelUpstreamPathReferences(...)` plus fixture coverage in `src/test/upstreamSubmoduleCutoverReadiness.test.ts` to automatically detect root-level references when an equivalent asset exists under `upstream/...`.
- [x] Rewrite references to point at `upstream/...` explicitly (including npm/yarn commands, fixture paths, and script helpers).
  - Updated npm container test commands in `package.json` to execute against `upstream/src/test/...` and `upstream/src/test/tsconfig.json`.
- [x] Introduce shared path helpers (where practical) to avoid hardcoded duplicate path strings in tests.
  - Added `src/spec-node/migration/upstreamPaths.ts` and adopted `buildUpstreamPath(...)` in cutover readiness tests.
- [x] Ensure CI jobs execute against `upstream/` sources and fail fast when submodule is missing/uninitialized.
  - Added `build/check-upstream-submodule.js` and `npm run check-upstream-submodule` so CI can fail fast when `upstream/` is missing/uninitialized.

### 3) Compatibility target versioning
- [x] Define the compatibility contract as: “this repo targets the exact commit pinned in `upstream/`.”
  - Added `resolvePinnedUpstreamCommit(...)` and `formatUpstreamCompatibilityContract(...)` helpers (with tests) to make the pinned-commit contract explicit and machine-resolvable.
- [ ] Expose the pinned upstream commit in test output/logging for traceability.
- [ ] Add a dedicated CI check that reports diffs/regressions when submodule commit changes.
- [ ] Create an “update upstream” workflow (bump submodule -> run parity suite -> fix breakages -> merge).

### 4) Documentation updates
- [ ] Update `README.md` with:
  - [ ] why `upstream/` exists,
  - [ ] how to clone/init submodules,
  - [ ] what to run when submodule is not initialized,
  - [ ] how compatibility testing maps to the pinned upstream revision.
- [ ] Add/update root `AGENTS.md` with contributor/agent rules for:
  - [ ] where upstream code must live (`upstream/` only),
  - [ ] where project-owned changes should be made,
  - [ ] how to perform/validate submodule bumps.
- [ ] Add a short migration note in changelog or docs index once root-level upstream code is removed.

### 5) Execution plan and rollout
- [ ] Land this as staged PRs to reduce risk:
  1. [ ] docs + guardrails (`README.md`, `AGENTS.md`, CI checks),
  2. [ ] path rewrites in tests/scripts,
  3. [ ] removal of duplicated root upstream code,
  4. [ ] final parity + cleanup.
- [ ] Run full parity/integration suite before and after each stage to isolate regressions.
- [ ] Gate final removal behind green CI across at least one Linux x64 lane.

### Exit criteria
- [ ] No tests/build scripts depend on root-level upstream copies.
- [ ] `upstream/` submodule commit is the declared compatibility baseline.
- [ ] Docs clearly explain contributor workflow for submodule init/update.
- [ ] CI protects against accidental drift or missing submodule checkout.
