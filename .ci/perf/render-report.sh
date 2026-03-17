#!/usr/bin/env bash
# Hardened wrapper for cockpit report rendering benchmark.
source "$(dirname "$0")/lib.sh"

BIN=$(perfgate_bin)
OUT_DIR=$(make_tempdir)

"$BIN" report \
  --compare .ci/fixtures/compare/compare-receipt.json \
  --out "$OUT_DIR/report.json" \
  >/dev/null
