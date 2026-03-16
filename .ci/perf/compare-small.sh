#!/bin/bash
set -e
if [ -f "./target/release/perfgate" ]; then BIN="./target/release/perfgate"; elif [ -f "./target/release/perfgate.exe" ]; then BIN="./target/release/perfgate.exe"; else echo "perfgate binary not found"; exit 1; fi

OUT_DIR=$(mktemp -d)
$BIN compare --baseline .ci/fixtures/compare/small-baseline.json --current .ci/fixtures/compare/small-current.json --out "$OUT_DIR/out.json" > /dev/null || true
rm -rf "$OUT_DIR"
