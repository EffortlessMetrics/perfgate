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

"$BIN" md \
  --compare .ci/fixtures/compare/compare-receipt.json \
  --out "$OUT_DIR/comment.md" \
  >/dev/null
