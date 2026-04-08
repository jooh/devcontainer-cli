# Native Compatibility Dashboard

- Pinned upstream commit: `39685cf1aa58b5b11e90085bd32562fad61f4103`
- Pinned spec commit: `c95ffeed1d059abfe9ffbe79762dc2fa4e7c2421`
- Command matrix source: `docs/upstream/command-matrix.json`
- Native parity inventory: `docs/upstream/parity-inventory.md`

## Current snapshot

- Declared upstream command paths present natively: `20/20`
- Upstream options with a native source reference in mapped Rust sources: `136/200`
- The parity inventory is a static source-evidence report. It is intended to identify obvious gaps and track drift, not to claim semantic parity by itself.

## Highest-Impact Gaps

- `build` and `up` now layer Features for image, dockerfile, and Docker Compose configs, but several upstream runtime flags are still only partially covered.
- `read-configuration --include-features-configuration` now resolves native Feature sets, but still relies on fixture/manual manifests rather than full OCI resolution.
- `outdated` and `upgrade` still rely on repo-owned fixture/manual catalog data instead of real upstream registry resolution.
- `features` and `templates` published flows still use local OCI layouts and embedded/local substitutes instead of real OCI registry fetch/publish behavior.
- Several upstream command flags remain unimplemented or only partially honored; see `docs/upstream/parity-inventory.md` for the per-command inventory.

## Guardrails

- `cargo test --manifest-path cmd/devcontainer/Cargo.toml`
- `npm test`
- `node build/generate-cli-reference.js --check`
- `node build/generate-parity-inventory.js --check`
- `node build/check-native-only.js`
- `node build/check-parity-harness.js`
- `node build/check-spec-drift.js`
- `node build/check-no-node-runtime.js`
