#!/usr/bin/env bash
set -euo pipefail

mkdir -p dist/standalone

# Phase-2 placeholder build contract:
# copy a runnable wrapper to the expected standalone artifact location.
# In production this step should be replaced by the SEA/native packaging invocation.
cat > dist/standalone/devcontainer-linux-x64 <<'BIN'
#!/usr/bin/env bash
set -euo pipefail
exec node "$(dirname "$0")/../spec-node/devContainersSpecCLI.js" "$@"
BIN
chmod +x dist/standalone/devcontainer-linux-x64
