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
- `node build/generate-cli-reference.js --check`

## Current parity scope

The automated repo-owned parity checks currently verify:

- command-matrix drift against pinned upstream CLI sources
- generated command-reference drift against the pinned upstream CLI sources
- schema drift against the pinned spec submodule
- native startup/help behavior without Node on `PATH`
- a narrow `read-configuration` parity slice using pinned fixtures and golden expectations

The native runtime now also has repo-owned Rust integration coverage for `build`, `up`, `set-up`, `run-user-commands`, and `exec` using a podman-compatible fake engine harness.

The remaining parity work is concentrated in four areas:

- Docker Compose parity for `build` and `up`
- upstream-equivalent merge/output behavior for `read-configuration`
- upstream-equivalent `outdated` and `upgrade` lockfile behavior
- OCI-backed `features` / `templates` subcommands and broader parity-harness coverage

The parity harness is intentionally narrow today. Expanding command coverage should happen by adding new scenarios and golden data under `src/test/parity/`.
