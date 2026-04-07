# Native distribution

## Release artifacts

- GitHub Releases is the active distribution channel.
- `.github/workflows/devcontainer-release.yml` builds release archives for Linux x64 (glibc), Linux x64 (musl), macOS x64, and macOS arm64.
- Each release artifact currently includes a compressed archive and a SHA-256 checksum file.

## Local build flow

- `scripts/standalone/build.sh <target>` builds the Rust release binary and places it under `dist/standalone/`.
- `scripts/standalone/build-linux-x64-musl.sh` builds the Linux x64 musl artifact for older-glibc distro compatibility.
- `scripts/standalone/smoke.sh <binary>` runs the repo-owned smoke commands against a built artifact.

## Current limitations

- The repository no longer ships or maintains the old bundled-Node installer path.
- Release automation does not currently sign artifacts or notarize macOS builds.
- Compatibility tooling in `package.json` is not part of the runtime distribution path.
