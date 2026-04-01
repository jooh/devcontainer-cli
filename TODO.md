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

## Implementation plan (phased)

### Phase 0 — Discovery and constraints (1 week)
- [ ] Inventory all Node-specific and native-binding dependencies (especially `node-pty`).
- [ ] Confirm target OS/arch matrix (Linux/macOS x64+arm64; Windows expectations).
- [ ] Define "fat binary" acceptance criteria:
  - [ ] single file on disk?
  - [ ] no external runtime?
  - [ ] startup latency target?
  - [ ] max binary size budget?
- [ ] Define parity scope for first release (which subcommands must be supported).

### Phase 1 — Fast standalone executable PoC (1–2 weeks)
- [ ] Prototype Node SEA (or alternative) from existing bundle.
- [ ] Validate command coverage:
  - [ ] `up`
  - [ ] `build`
  - [ ] `exec`
  - [ ] `read-configuration`
  - [ ] `features` and `templates` core commands
- [ ] Validate behavior on Docker + Docker Compose in CI-like environment.
- [ ] Identify blockers around native addons / dynamic requires.
- [ ] Produce size/startup benchmarks and compare to current install script approach.

### Phase 2 — Productionize short-term binary distribution (2–4 weeks)
- [ ] Add reproducible build pipeline for standalone binary artifacts.
- [ ] Add signing/notarization strategy where needed.
- [ ] Add smoke/integration test lane that runs packaged executable (not just `node ...`).
- [ ] Add release docs and fallback installer path.
- [ ] Publish experimental channel (e.g., `-standalone` artifacts).

### Phase 3 — Native rewrite foundation (Rust) (2–4 weeks)
- [ ] Create `cmd/devcontainer-native` Rust crate in repo (or sibling repo with mirrored CI).
- [ ] Implement CLI argument surface for top-level commands and help text parity.
- [ ] Implement logging format parity (`text` / `json`) and exit code semantics.
- [ ] Add compatibility bridge:
  - [ ] If command not yet ported, shell out to current Node implementation.

### Phase 4 — Port high-value command paths first (6–12+ weeks)
- [ ] Port read-only/introspection paths first:
  - [ ] `read-configuration`
  - [ ] portions of metadata/resolve logic
- [ ] Port execution paths next:
  - [ ] `build`
  - [ ] `up`
  - [ ] `exec`
- [ ] Port `features`/`templates` subcommands.
- [ ] Preserve compatibility output JSON schema and text output where practical.

### Phase 5 — Hardening and cutover
- [ ] Full integration parity suite against Node baseline.
- [ ] Performance and resource benchmarking.
- [ ] Release native binary as default, keep Node build as fallback for one major cycle.
- [ ] Deprecate and remove fallback once confidence is high.

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
- [ ] Build a minimal standalone executable PoC from current Node bundle.
- [ ] Run top 5 commands against existing test fixtures.
- [ ] Create a short decision memo: SEA viability vs packager alternatives.
- [ ] Decide whether to launch Rust foundation in parallel immediately or after PoC sign-off.
