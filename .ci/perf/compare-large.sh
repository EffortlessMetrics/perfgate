#!/usr/bin/env bash
# Hardened wrapper for large comparison benchmark.
source "$(dirname "$0")/lib.sh"

BIN=$(perfgate_bin)
OUT_DIR=$(make_tempdir)

allow_policy_exit "$BIN" compare \
  --baseline .ci/fixtures/compare/large-baseline.json \
  --current .ci/fixtures/compare/large-current.json \
  --out "$OUT_DIR/out.json" \
  >/dev/null
