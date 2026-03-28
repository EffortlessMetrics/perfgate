# Release Readiness: v0.15.1

Last verified: 2026-03-28 on `main` after merging the `0.15.1` prep work.

## Patch Scope

This patch release is intentionally narrow:
- restore local `perfgate serve` baseline workflows (`promote --to-server`,
  `baseline list`, `baseline history`, `compare --baseline @server:<bench>`)
- align baseline-service docs with the actual `0.15.x` command and server
  surface
- roll examples and release docs forward to `v0.15.1`

## Current Status

- Workspace and internal crate versions are set to `0.15.1` on `main`.
- The local-mode baseline fix and doc cleanup are merged on `main`.
- `cargo run -p xtask -- ci` passed locally on 2026-03-28 against `main`.
- `v0.15.1` has not been tagged yet.

## Tested and Working

These commands were tested end-to-end on Windows (x86_64, Rust 1.92):

| Command | Status | Notes |
|---------|--------|-------|
| `run` | **Works** | Clean receipts, Windows IO metrics collected |
| `compare` | **Works** | Correct deltas, exit codes |
| `md` | **Works** | Clean Markdown table output |
| `github-annotations` | **Works** | Emits correct annotation format |
| `report` | **Works** | Generates perfgate.report.v1 |
| `promote` | **Works** | Copies and normalizes receipts |
| `export --format csv` | **Works** | Correct CSV with headers |
| `export --format jsonl` | **Works** | Valid JSON per line |
| `export --format junit` | **Works** | Valid XML |
| `export --format html` | **Works** | Valid HTML table |
| `export --format prometheus` | **Works** | Valid text exposition format |
| `check` | **Works** | Config-driven, finds baselines via `baseline_pattern` |
| `paired` | **Works** | Interleaved execution, produces compare receipt |
| `summary` | **Works** | Terminal table from compare receipts |
| `aggregate` | **Works** | Merges run receipts |
| `explain` | **Works** | Generates diagnostic text |
| `blame` | **Works** | Diff two Cargo.lock files |
| `bisect` | **Not tested** | Wraps git bisect, requires repo with history |
| `serve` | **Works** | Local SQLite dashboard/server; local baseline workflows re-verified |
| `baseline upload` | **Works** | Requires `pg_live_` key with 32+ char suffix |
| `baseline list` | **Works** | Lists uploaded baselines correctly |
| `baseline download` | **Works** | Note: uses `--output` not `--out` |
| `baseline history` | **Works** | Local-mode smoke flow re-verified in 0.15.1 prep |
| `baseline delete/verdicts` | **Not tested** | Server is functional, but these were not re-run for this patch |
| `check --mode cockpit` | **Works** | Produces sensor.report.v1 envelope + extras |

## Known Bugs

- [#55](https://github.com/EffortlessMetrics/perfgate/issues/55) ~~Leftover DEBUG prints~~ — **Fixed** (committed)
- [#56](https://github.com/EffortlessMetrics/perfgate/issues/56) ~~CLI examples in docs use wrong flags~~ — **Fixed** (committed)
- [#58](https://github.com/EffortlessMetrics/perfgate/issues/58) Server `--api-keys` glob `*` pattern causes 500 errors (use `.*` as workaround)

## Doc/Flag Mismatches Found During Testing (all fixed)

### `paired` command
- Docs said `--threshold 0.20` — **fixed** to `--fail-on-regression 20.0`
- Docs omitted required `--name` flag — **fixed**

### `blame` command
- Docs say `--compare cmp.json` — **wrong**. Actual: `--baseline <Cargo.lock> --current <Cargo.lock>`

### `bisect` command
- Docs say `--bench my-bench --config perfgate.toml` — **wrong**. Actual: `--good <COMMIT> --executable <PATH>`

### `baseline download` command
- Docs say `--out` — **wrong**, actual flag is `--output`

### `check` with `baseline_dir`
- `baseline_dir` may have path resolution issues in mixed Unix/Windows environments (MSYS2). `baseline_pattern` with absolute path works reliably.

### `run -p perfgate` vs `run -p perfgate-cli`
- `cargo run -p perfgate` fails (no bin target — it's a library facade). **Fixed**: all docs now use `cargo run -p perfgate-cli --`.

## What's Solid (ship with confidence)

The **core local gating pipeline** is production-quality:
- `run` → `compare` → `md`/`report` → `promote`
- `check` (config-driven single command)
- `paired` (noise-resistant benchmarking)
- All export formats
- Exit code contract (0/1/2/3)
- JSON receipt versioning
- Host fingerprinting
- Statistical significance (Welch's t-test)

## What's Functional But Needs Hardening

- **Baseline server** — works for dev/small-team. Storage backends (SQLite, PostgreSQL, S3) are implemented. Not load-tested. GitHub Actions OIDC is exercised; GitLab and custom OIDC exist but remain lightly exercised.
- **`bisect`** — wraps git bisect. Works in concept but depends on repo structure and build system. Edge cases likely.
- **`explain`** — generates prompts, doesn't call an LLM. Useful but the name oversells it.
- **`aggregate`** — simple merge, no weighting or outlier detection.

## What's Missing / Not Built Yet

- **Windows timeout support** — returns `AdapterError::TimeoutUnsupported`
- **Windows page_faults/ctx_switches** — not collected
- **API key management CLI** — auth types exist, no key CRUD
- **Audit logging** — verdict history exists, no modification audit trail
- **OIDC beyond GitHub Actions** — GitLab/Okta not tested
- **Executable doc tests** — CLI examples aren't validated in CI
- **`cargo run -p perfgate` ergonomics** — doesn't work without specifying `--bin`

## Recommended Release Approach

1. **Tag `v0.15.1`** — trigger the binary release workflow from the verified `main` state
2. **Verify the GitHub release** — confirm the binary artifacts and checksums upload successfully
3. **Follow up** — crates.io publish, versioned action tag, workflow action runtime upgrades
