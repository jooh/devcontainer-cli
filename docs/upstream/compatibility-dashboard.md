# Native Compatibility Dashboard

- Pinned upstream commit: `39685cf1aa58b5b11e90085bd32562fad61f4103`
- Pinned spec commit: `c95ffeed1d059abfe9ffbe79762dc2fa4e7c2421`
- Command matrix source: `docs/upstream/command-matrix.json`

## Native command status

- `read-configuration`: parity-tested for the current repo-owned scenarios; broader upstream merge parity is still pending
- `build`: native runtime foundation for image/dockerfile flows; Docker Compose parity is still pending
- `up`: native runtime foundation for image/dockerfile flows; Docker Compose parity is still pending
- `set-up`: native lifecycle foundation for existing containers
- `run-user-commands`: native lifecycle foundation for existing containers
- `outdated`: partial native implementation
- `upgrade`: partial native implementation
- `exec`: native in-container execution foundation
- `features`: native local flows, `test` and OCI parity still pending
- `templates`: native local flows, OCI parity still pending

## Guardrails

- `cargo test --manifest-path cmd/devcontainer/Cargo.toml`
- `npm test`
- `node build/generate-cli-reference.js --check`
- `node build/check-native-only.js`
- `node build/check-parity-harness.js`
- `node build/check-spec-drift.js`
- `node build/check-no-node-runtime.js`
