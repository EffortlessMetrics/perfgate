#!/usr/bin/env bash
# Hardened wrapper for single check benchmark.
source "$(dirname "$0")/lib.sh"

BIN=$(perfgate_bin)
make_tempdir OUT_DIR

allow_policy_exit "$BIN" check \
  --config .ci/fixtures/check/perfgate.toml \
  --bench test-bench \
  --out-dir "$OUT_DIR" \
  >/dev/null
