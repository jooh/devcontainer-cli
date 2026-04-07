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

The native runtime now also has repo-owned Rust integration coverage for `build`, `up`, `set-up`, `run-user-commands`, and `exec` using a podman-compatible fake engine harness, including Docker Compose project-name and existing-container coverage for `build` and `up`.

The original umbrella gaps around `read-configuration`, `outdated` / `upgrade`, deeper Docker Compose support, and OCI-oriented `features` / `templates` flows are now covered by native code paths with repo-owned tests.

What remains is incremental hardening rather than missing command families:

- broader parity-harness coverage beyond the current pinned scenarios
- additional upstream edge cases where we decide the maintenance cost is worth the extra compatibility surface
