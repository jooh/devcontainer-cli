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
- pinned `read-configuration` parity scenarios, including upstream-style workspace output

The native runtime now also has repo-owned Rust integration coverage for `build`, `up`, `set-up`, `run-user-commands`, and `exec` using a podman-compatible fake engine harness, including a basic Docker Compose lane for `build` and `up`.

The remaining parity work is concentrated in two areas:

- deeper Docker Compose parity for `build` and `up` beyond the current basic service foundation
- OCI-backed `features` / `templates` subcommands and broader parity-harness coverage

The parity harness is intentionally narrow today. Expanding command coverage should happen by adding new scenarios and golden data under `src/test/parity/`.
