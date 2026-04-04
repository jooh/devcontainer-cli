#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <target>" >&2
  echo "example targets: linux-x64, darwin-x64, darwin-arm64" >&2
  exit 2
fi

target="$1"
output="dist/standalone/devcontainer-${target}"

mkdir -p dist/standalone

cargo build --release --manifest-path cmd/devcontainer/Cargo.toml
cp cmd/devcontainer/target/release/devcontainer "$output"
chmod +x "$output"
