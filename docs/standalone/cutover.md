# Runtime cutover status

- The distributed CLI runtime is native Rust only.
- Runtime invocation of Node is forbidden; `build/check-no-node-runtime.js` enforces that contract.
- `DEVCONTAINER_NATIVE_ONLY=1` remains a local regression guard for unsupported paths.

## Active guardrails

- `cargo fmt --manifest-path cmd/devcontainer/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path cmd/devcontainer/Cargo.toml -- -D warnings`
- `cargo test --manifest-path cmd/devcontainer/Cargo.toml`
- `node build/check-native-only.js`
- `node build/check-no-node-runtime.js`
- `node build/check-parity-harness.js`

## Current parity scope

The automated repo-owned parity checks currently verify:

- command-matrix drift against pinned upstream CLI sources
- schema drift against the pinned spec submodule
- native startup/help behavior without Node on `PATH`
- a repo-owned `read-configuration` parity scenario using pinned fixtures and golden expectations

The parity harness is intentionally narrow today. Expanding command coverage should happen by adding new scenarios and golden data under `src/test/parity/`.
