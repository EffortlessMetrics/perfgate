#!/bin/bash
set -e
if [ -f "./target/release/perfgate" ]; then BIN="./target/release/perfgate"; elif [ -f "./target/release/perfgate.exe" ]; then BIN="./target/release/perfgate.exe"; else echo "perfgate binary not found"; exit 1; fi

OUT_DIR=$(mktemp -d)
$BIN md --compare .ci/fixtures/compare/compare-receipt.json --out "$OUT_DIR/comment.md" > /dev/null || true
rm -rf "$OUT_DIR"
