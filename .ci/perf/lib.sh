#!/usr/bin/env bash
set -euo pipefail

# Resolves the path to the perfgate release binary.
perfgate_bin() {
  if [ -f "./target/release/perfgate" ]; then
    printf '%s\n' "./target/release/perfgate"
  elif [ -f "./target/release/perfgate.exe" ]; then
    printf '%s\n' "./target/release/perfgate.exe"
  else
    echo "perfgate binary not found" >&2
    exit 1
  fi
}

# Creates a temporary directory and sets up a trap to remove it on exit.
# Outputs the path to the created directory.
make_tempdir() {
  local dir
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' EXIT
  printf '%s\n' "$dir"
}

# Executes a command and allows policy-driven exit codes (0, 2, 3).
# Any other exit code will cause the script to exit with that status.
allow_policy_exit() {
  set +e
  "$@"
  local status=$?
  set -e
  case "$status" in
    0|2|3) return 0 ;;
    *) return "$status" ;;
  esac
}
