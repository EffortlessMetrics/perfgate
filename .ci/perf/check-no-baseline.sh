#!/usr/bin/env bash
set -euo pipefail

if [ -f "./target/release/perfgate" ]; then
  BIN="./target/release/perfgate"
elif [ -f "./target/release/perfgate.exe" ]; then
  BIN="./target/release/perfgate.exe"
else
  echo "perfgate binary not found" >&2
  exit 1
fi

OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

set +e
"$BIN" check \
  --config .ci/fixtures/check/perfgate.toml \
  --bench test-no-baseline \
  --out-dir "$OUT_DIR" \
  >/dev/null
status=$?
set -e

case "$status" in
  0|2|3) exit 0 ;;
  *) exit "$status" ;;
esac
