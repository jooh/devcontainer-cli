#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <target>" >&2
  echo "example targets: linux-x64, darwin-x64, darwin-arm64" >&2
  exit 2
fi

target="$1"
output="dist/standalone/devcontainer-${target}"

mkdir -p dist/standalone/spec-node

# Standalone distribution placeholder build contract:
# package a runnable wrapper plus the JS payload to emulate a standalone artifact.
# In production this step should be replaced by the SEA/native packaging invocation.
cp -R dist/spec-node/. dist/standalone/spec-node/
cat > "$output" <<'BIN'
#!/usr/bin/env bash
set -euo pipefail
exec node "$(dirname "$0")/spec-node/devContainersSpecCLI.js" "$@"
BIN
chmod +x "$output"
