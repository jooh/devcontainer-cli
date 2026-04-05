# Native Compatibility Dashboard

- Pinned upstream commit: `39685cf1aa58b5b11e90085bd32562fad61f4103`
- Pinned spec commit: `c95ffeed1d059abfe9ffbe79762dc2fa4e7c2421`
- Command matrix source: `docs/upstream/command-matrix.json`

## Native command status

- `read-configuration`: native
- `build`: native
- `up`: native
- `set-up`: native
- `run-user-commands`: native
- `outdated`: native
- `upgrade`: native
- `exec`: native
- `features`: native local flows
- `templates`: native local flows

## Guardrails

- `cargo test --manifest-path cmd/devcontainer/Cargo.toml`
- `npm test`
- `node build/check-native-only.js`
- `node build/check-parity-harness.js`
- `node build/check-spec-drift.js`
- `node build/check-no-node-runtime.js`
