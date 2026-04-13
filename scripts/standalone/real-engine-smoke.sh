#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <standalone-binary-path>" >&2
  exit 2
fi

binary="$1"
if [[ ! -x "$binary" ]]; then
  echo "standalone binary not found or not executable: $binary" >&2
  exit 2
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required for real-engine smoke" >&2
  exit 2
fi

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq -- "$expected" "$file"; then
    echo "expected '$expected' in $file" >&2
    cat "$file" >&2
    exit 1
  fi
}

tmp_dir="$(mktemp -d)"
workspace="$tmp_dir/workspace"
container_id=""

cleanup() {
  if [[ -n "$container_id" ]]; then
    docker rm -f "$container_id" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

mkdir -p "$workspace/.devcontainer"
cat >"$workspace/.devcontainer/devcontainer.json" <<'EOF'
{
  "image": "alpine:3.20",
  "workspaceFolder": "/workspace",
  "updateRemoteUserUID": false,
  "onCreateCommand": "printf on-create > /workspace/.on-create",
  "updateContentCommand": "printf update-content > /workspace/.update-content",
  "postCreateCommand": "printf ready > /workspace/.ready",
  "postStartCommand": "printf started > /workspace/.started",
  "postAttachCommand": "printf attached > /workspace/.attached"
}
EOF

"$binary" up --workspace-folder "$workspace" >"$tmp_dir/up.json"
assert_file_contains "$tmp_dir/up.json" '"outcome":"success"'
container_id="$(sed -n 's/.*"containerId":"\([^"]*\)".*/\1/p' "$tmp_dir/up.json")"
if [[ -z "$container_id" ]]; then
  echo "container id missing from up output" >&2
  cat "$tmp_dir/up.json" >&2
  exit 1
fi

for marker in .on-create .update-content .ready .started .attached; do
  if [[ ! -f "$workspace/$marker" ]]; then
    echo "expected lifecycle marker $marker" >&2
    ls -la "$workspace" >&2
    exit 1
  fi
done

exec_output="$("$binary" exec --workspace-folder "$workspace" /bin/cat /workspace/.ready)"
if [[ "$exec_output" != "ready" ]]; then
  echo "unexpected exec output: $exec_output" >&2
  exit 1
fi

"$binary" run-user-commands --workspace-folder "$workspace" >"$tmp_dir/run-user-commands.json"
assert_file_contains "$tmp_dir/run-user-commands.json" '"outcome":"success"'

"$binary" set-up --workspace-folder "$workspace" >"$tmp_dir/set-up.json"
assert_file_contains "$tmp_dir/set-up.json" '"outcome":"success"'
