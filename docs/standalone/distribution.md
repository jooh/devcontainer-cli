# Standalone distribution report (in progress)

Status updated: 2026-04-03

## Reproducible standalone build pipeline
- Added a deterministic standalone release workflow for Linux x64 and macOS (x64 + arm64) artifacts.
- Build inputs are pinned and reproducibility is enforced via deterministic bundle + artifact checksums.
- Workflow path: `.github/workflows/devcontainer-release.yml`.

## Signing strategy
- Standalone artifacts are signed using keyless Sigstore/Cosign in CI.
- Public checksums and signature material are published with each standalone release target.
- macOS notarization remains deferred while the channel is marked experimental.

## Packaged executable smoke/integration lane
- Added a native-only startup contract lane that executes the Rust binary with `PATH` excluding Node.
- Required smoke commands include:
  - `read-configuration`
  - `build`
  - `up`
  - `exec`

## Release docs and fallback installer path
- Standalone release guidance documents artifact usage, verification, and known limitations.
- Fallback installer remains the npm path:
  - `npm i -g @devcontainers/cli`

## Experimental publication channel
- Standalone artifacts are published on an experimental channel using `-standalone` naming.
- Channel is intentionally marked experimental while cross-platform support and TTY parity mature.
