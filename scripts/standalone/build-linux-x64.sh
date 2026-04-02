#!/usr/bin/env bash
set -euo pipefail

mkdir -p dist/standalone/spec-node

# Standalone distribution placeholder build contract:
# package a runnable wrapper plus the JS payload to emulate a standalone artifact.
# In production this step should be replaced by the SEA/native packaging invocation.
cp -R dist/spec-node/. dist/standalone/spec-node/
cat > dist/standalone/devcontainer-linux-x64 <<'BIN'
#!/usr/bin/env bash
set -euo pipefail
exec node "$(dirname "$0")/spec-node/devContainersSpecCLI.js" "$@"
BIN
chmod +x dist/standalone/devcontainer-linux-x64
