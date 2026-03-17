#!/usr/bin/env bash
# Hardened wrapper for markdown rendering benchmark.
source "$(dirname "$0")/lib.sh"

BIN=$(perfgate_bin)
OUT_DIR=$(make_tempdir)

"$BIN" md \
  --compare .ci/fixtures/compare/compare-receipt.json \
  --out "$OUT_DIR/comment.md" \
  >/dev/null
