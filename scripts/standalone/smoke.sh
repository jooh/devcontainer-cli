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

"$binary" --help >/dev/null
"$binary" read-configuration --help >/dev/null
"$binary" up --help >/dev/null
"$binary" build --help >/dev/null
"$binary" exec --help >/dev/null
