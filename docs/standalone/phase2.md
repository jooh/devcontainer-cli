# Phase 2 — Productionize short-term binary distribution (completed)

Date completed: 2026-04-01

## Reproducible standalone build pipeline
- Added a deterministic standalone release workflow for Linux x64 artifacts.
- Build inputs are pinned and reproducibility is enforced via deterministic bundle + artifact checksums.
- Workflow path: `.github/workflows/standalone-release.yml`.

## Signing strategy
- Linux standalone artifacts are signed using keyless Sigstore/Cosign in CI.
- Public checksums and signature material are published with each standalone release.
- Notarization remains deferred for non-Linux targets while Linux x64 is the only supported standalone channel.

## Packaged executable smoke/integration lane
- Added a `standalone-smoke` lane that executes packaged binary tests (instead of `node ...`).
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
