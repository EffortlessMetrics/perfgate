# perfgate Requirements

This document specifies the functional requirements for perfgate commands, artifacts, and behaviors.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## External Interface Contracts

Receipt schemas are public API. The following schema IDs are stable:
- `perfgate.run.v1`
- `perfgate.compare.v1`
- `perfgate.report.v1`
- `perfgate.config.v1`
- `sensor.report.v1` (cockpit mode envelope, vendored at `contracts/schemas/`)
- `perfgate.baseline.v1` (baseline server record, vendored at `contracts/schemas/`)

Within a `v1` schema, changes MUST be additive and backward compatible. Fields, codes, and reason tokens MUST NOT be renamed or repurposed.

CLI surface stability: the following commands are considered stable and MUST remain available in v2:
- `run`
- `compare`
- `report`
- `md`
- `github-annotations`
- `check`
- `promote`
- `export`
- `paired`
- `baseline` (Added in v2.0)

## Commands

perfgate provides ten commands for the performance budget workflow.

### run

Executes a command repeatedly and emits a run receipt.

**Required Arguments:**
- `--name`: Bench identifier (used for baselines and reporting)
- `-- <command>`: Command to execute (argv, no shell parsing)

**Optional Arguments:**
- `--repeat` (default: 5): Number of measured samples
- `--warmup` (default: 0): Warmup samples excluded from stats
- `--work`: Units of work per run (enables `throughput_per_s`)
- `--cwd`: Working directory for command execution
- `--timeout`: Per-run timeout (e.g., "2s")
- `--env`: Environment variables (repeatable, KEY=VALUE format)
- `--output-cap-bytes` (default: 8192): Max bytes captured from stdout/stderr
- `--allow-nonzero`: Do not fail when command returns nonzero
- `--include-hostname-hash`: Include SHA-256 hashed hostname in host fingerprint
- `--out` (default: "perfgate.json"): Output file path
- `--pretty`: Pretty-print JSON output

**Behavior:**
- The command MUST execute `warmup + repeat` iterations
- Warmup samples MUST be marked with `warmup: true` and excluded from statistics
- Statistics MUST be computed from non-warmup samples only
- If any non-warmup sample times out or returns nonzero (without `--allow-nonzero`), the command SHALL exit 1 after writing the receipt
- Output MUST conform to `perfgate.run.v1` schema

### compare

Compares a current run receipt against a baseline.

**Required Arguments:**
- `--baseline`: Path to baseline run receipt (or `--baseline-server`)
- `--current`: Path to current run receipt

**Optional Arguments:**
- `--baseline-server`: Fetch baseline from centralized server (requires config)
- `--baseline-version`: Specific version to fetch from server
- `--threshold` (default: 0.20): Global regression threshold (fraction)
- `--warn-factor` (default: 0.90): Warn threshold = threshold * warn_factor
- `--metric-threshold`: Per-metric threshold override (e.g., `wall_ms=0.10`)
- `--direction`: Per-metric direction override (e.g., `throughput_per_s=higher`)
- `--metric-stat`: Per-metric statistic override (`median` or `p95`)
- `--significance-alpha`: Optional p-value threshold for Welch's t-test
- `--significance-min-samples` (default: 8): Minimum per-side sample count before significance is computed
- `--require-significance`: Require significance for warn/fail escalation when `--significance-alpha` is set
- `--fail-on-warn`: Treat warn verdict as exit 3
- `--host-mismatch` (default: "warn"): Host mismatch policy (`warn`, `error`, `ignore`)
- `--out` (default: "perfgate-compare.json"): Output file path
- `--pretty`: Pretty-print JSON output

**Behavior:**
- Budgets MUST be built for metrics present in both baseline and current
- `wall_ms` MUST always be included as a candidate metric
- Comparison MUST use median values by default, with optional per-metric overrides (e.g., `p95`)
- Verdict reasons MUST be stable tokens (e.g., `wall_ms_warn`, `wall_ms_fail`)
- Output MUST conform to `perfgate.compare.v1` schema

### md

Renders a Markdown summary from a compare receipt.

**Required Arguments:**
- `--compare`: Path to compare receipt

**Optional Arguments:**
- `--out`: Output file path (default: stdout)

**Behavior:**
- Output MUST include verdict header with emoji (pass/warn/fail)
- Output MUST include a table with all metrics, values, deltas, and status
- Output MUST include verdict reason tokens if any exist

### github-annotations

Emits GitHub Actions annotations from a compare receipt.

**Required Arguments:**
- `--compare`: Path to compare receipt

**Behavior:**
- MUST emit `::error::` annotations for metrics with Fail status
- MUST emit `::warning::` annotations for metrics with Warn status

### report

Generates a cockpit-compatible report from a compare receipt.

**Required Arguments:**
- `--compare`: Path to compare receipt

**Optional Arguments:**
- `--out` (default: "perfgate-report.json"): Output file path
- `--md`: Also write markdown summary to this path
- `--pretty`: Pretty-print JSON output

**Behavior:**
- Output MUST conform to `perfgate.report.v1` schema
- Report verdict MUST match compare verdict
- Findings MUST be ordered deterministically by metric name

### check

Config-driven one-command workflow.

**Required Arguments:**
- `--bench`: Name of the benchmark to run (must match `[[bench]]` in config)

**Optional Arguments:**
- `--config` (default: "perfgate.toml"): Path to config file (TOML or JSON)
- `--out-dir` (default: "artifacts/perfgate"): Output directory for artifacts
- `--baseline`: Path/URI to baseline file (overrides config default)
- `--baseline-server`: Use centralized server for baselines
- `--require-baseline`: Fail if baseline is missing (default: warn and continue)
- `--fail-on-warn`: Treat warn verdict as exit 3
- `--mode` (default: "standard"): Output mode (`standard` or `cockpit`)
- `--all`: Run all benchmarks defined in config (multi-bench mode)
- `--output-github`: Write verdict/count outputs to `$GITHUB_OUTPUT`

**Behavior:**
- MUST load config file and find bench by name (or run all with `--all`)
- MUST run the benchmark using config parameters
- MUST write all artifacts to `out_dir`
- Baseline resolution order MUST be: `--baseline` > `baseline_server` (if enabled) > `defaults.baseline_pattern` > `defaults.baseline_dir` > `baselines/{bench}.json`
- If baseline exists, MUST compare and generate report
- In cockpit mode, MUST exit 0 unless catastrophic failure (e.g., I/O error)

### promote

Promotes a run receipt to become the new baseline.

**Required Arguments:**
- `--current`: Path/URI to the run receipt to promote
- `--to`: Path/URI where the baseline should be written (or `--to-server`)

**Optional Arguments:**
- `--to-server`: Upload baseline to centralized server
- `--normalize`: Strip run-specific fields for stable baselines
- `--version`: Explicit version for server upload
- `--tag`: Tags for server upload (repeatable)

**Behavior:**
- Without `--normalize`, receipt MUST be copied unchanged
- With `--normalize`:
  - `run.id` replaced with "baseline", timestamps reset to Unix epoch
- Locations MAY be local paths, cloud URIs (`s3://...`), or the Baseline Server

### export

Exports receipts to multiple formats for external analysis.

**Required Arguments (mutually exclusive):**
- `--run`: Path to run receipt
- `--compare`: Path to compare receipt

**Required Arguments:**
- `--out`: Output file path

**Optional Arguments:**
- `--format` (default: "csv"): Output format (`csv`, `jsonl`, `html`, `prometheus`)

**Behavior:**
- **CSV**: RFC 4180 compliant with header row
- **JSONL**: One JSON object per line
- **HTML**: Tabular summary representation
- **Prometheus**: Text exposition format with deterministic labels
- Output MUST be deterministic (stable metric ordering)

### paired

Paired benchmarking with interleaved baseline/current runs.

**Required Arguments:**
- `--baseline-cmd`: Baseline command (shell string)
- `--current-cmd`: Current command (shell string)

**Behavior:**
- MUST execute baseline and current commands alternately (B, C, B, C, ...)
- MUST measure each pair back-to-back to minimize environmental variance
- Output MUST conform to `perfgate.compare.v1` schema

### baseline (New in v2.0)

Dedicated baseline management commands for the Centralized Baseline Service.

**Subcommands:**
- `list`: List baselines for a project with optional filters (tags, git ref)
- `download`: Download a specific baseline version to a local file
- `upload`: Manually upload a run receipt as a baseline
- `delete`: Delete a specific baseline version
- `history`: View version history for a benchmark
- `rollback`: Revert a benchmark to a previous version

**Behavior:**
- MUST interact with the server defined in `perfgate.toml` or via environment variables
- MUST support authentication via API Key or JWT

## Cockpit Mode

The `check` command supports `--mode cockpit` for integration with monitoring dashboards.

**Behavior:**
- Output `report.json` MUST conform to `sensor.report.v1` schema
- Extras artifacts MUST use versioned names: `perfgate.run.v1.json`, etc.
- Always exits 0 for budget violations (recorded in report)
- Artifacts MUST be sorted by `(type, path)` for deterministic output

**Cockpit Mode Artifact Layout:**
```
artifacts/perfgate/
├── report.json                         # sensor.report.v1 envelope
├── comment.md
└── extras/
    ├── perfgate.run.v1.json
    ├── perfgate.compare.v1.json        (if baseline)
    └── perfgate.report.v1.json
```

## Baseline Server API (v2.0)

Centralized management service for fleet-scale performance monitoring.

**Key Requirements:**
- **Project Isolation**: Data isolated by project namespaces
- **Versioning**: Immutable versions for every baseline upload
- **Promotion**: Atomic promotion of "candidate" baselines to "production" status
- **Graceful Fallback**: Client MUST fall back to local/cloud storage if server is down
- **Auth**: Support for static API Keys and OIDC-based short-lived tokens

## Host Mismatch Detection

When comparing runs from different hosts, perfgate detects inconsistencies.

**Detection criteria:**
- Different `os`, `arch`, `cpu_count`, or `hostname_hash`

**Policy (`--host-mismatch`):**
- `warn` (default): Emit warning, continue
- `error`: Exit 1 on mismatch
- `ignore`: Silently allow comparison

## System Metrics

On Unix (via `rusage`):
- `cpu_ms`, `max_rss_kb`, `page_faults`, `ctx_switches`

On Windows:
- `cpu_ms`, `max_rss_kb` (best-effort)

`binary_bytes`: Executable size tracking (cross-platform best-effort)

## Exit Codes

| Code | Meaning | Description |
|------|---------|-------------|
| `0` | Success | Command completed; pass verdict; or cockpit mode budget fail |
| `1` | Tool error | I/O errors, parse failures, missing required arguments |
| `2` | Policy fail | Budget violated (standard mode) |
| `3` | Warn as failure | Warn verdict with `--fail-on-warn` flag |
| `4` | Server error | Centralized Baseline Server returned error |
