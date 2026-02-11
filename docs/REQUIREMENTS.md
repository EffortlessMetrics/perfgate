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

Within a `v1` schema, changes MUST be additive and backward compatible. Fields, codes, and reason tokens MUST NOT be renamed or repurposed.

CLI surface stability: the following commands are considered stable and MUST remain available in v1:
- `run`
- `compare`
- `report`
- `md`
- `github-annotations`
- `check`
- `promote`
- `export`
- `paired`

## Commands

perfgate provides nine commands for the performance budget workflow.

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
- `--baseline`: Path to baseline run receipt
- `--current`: Path to current run receipt

**Optional Arguments:**
- `--threshold` (default: 0.20): Global regression threshold (fraction)
- `--warn-factor` (default: 0.90): Warn threshold = threshold * warn_factor
- `--metric-threshold`: Per-metric threshold override (e.g., `wall_ms=0.10`)
- `--direction`: Per-metric direction override (e.g., `throughput_per_s=higher`)
- `--fail-on-warn`: Treat warn verdict as exit 3
- `--host-mismatch` (default: "warn"): Host mismatch policy (`warn`, `error`, `ignore`)
- `--out` (default: "perfgate-compare.json"): Output file path
- `--pretty`: Pretty-print JSON output

**Behavior:**
- Budgets MUST be built for metrics present in both baseline and current
- `wall_ms` MUST always be included as a candidate metric
- `max_rss_kb` MUST be included only if present in both receipts
- `throughput_per_s` MUST be included only if present in both receipts
- Comparison MUST use median values for all metrics
- Verdict reasons MUST be stable tokens (e.g., `wall_ms_warn`, `wall_ms_fail`)
- Output MUST conform to `perfgate.compare.v1` schema

**Exit Codes:**
- Exit 0: Pass verdict (or warn without `--fail-on-warn`)
- Exit 2: Fail verdict (budget violated)
- Exit 3: Warn verdict with `--fail-on-warn`

### md

Renders a Markdown summary from a compare receipt.

**Required Arguments:**
- `--compare`: Path to compare receipt

**Optional Arguments:**
- `--out`: Output file path (default: stdout)

**Behavior:**
- Output MUST include verdict header with emoji (pass/warn/fail)
- Output MUST include bench name
- Output MUST include a table with all metrics, values, deltas, and status
- Output MUST include verdict reason tokens if any exist

### github-annotations

Emits GitHub Actions annotations from a compare receipt.

**Required Arguments:**
- `--compare`: Path to compare receipt

**Behavior:**
- MUST emit `::error::` annotations for metrics with Fail status
- MUST emit `::warning::` annotations for metrics with Warn status
- MUST NOT emit annotations for metrics with Pass status
- Each annotation MUST include bench name, metric name, and delta percentage

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
- Finding count MUST equal warn + fail count from deltas
- Summary counts MUST match verdict counts
- Findings MUST be ordered deterministically by metric name

### check

Config-driven one-command workflow.

**Required Arguments:**
- `--bench`: Name of the benchmark to run (must match `[[bench]]` in config)

**Optional Arguments:**
- `--config` (default: "perfgate.toml"): Path to config file (TOML or JSON)
- `--out-dir` (default: "artifacts/perfgate"): Output directory for artifacts
- `--baseline`: Path to baseline file (overrides config default)
- `--require-baseline`: Fail if baseline is missing (default: warn and continue)
- `--fail-on-warn`: Treat warn verdict as exit 3
- `--env`: Environment variables (repeatable)
- `--output-cap-bytes` (default: 8192): Max bytes captured
- `--allow-nonzero`: Do not fail when command returns nonzero
- `--mode` (default: "standard"): Output mode (`standard` or `cockpit`)
- `--all`: Run all benchmarks defined in config (multi-bench mode)
- `--pretty`: Pretty-print JSON output

**Behavior:**
- MUST load config file and find bench by name (or run all with `--all`)
- MUST run the benchmark using config parameters
- MUST write all artifacts to `out_dir`
- If baseline exists, MUST compare and generate report
- If baseline missing without `--require-baseline`, MUST warn and exit 0
- If baseline missing with `--require-baseline`, MUST exit 1

**Exit Codes:**
- Exit 0: Pass (or warn without `--fail-on-warn`, or no baseline without `--require-baseline`)
- Exit 1: Tool error or baseline required but missing
- Exit 2: Fail verdict
- Exit 3: Warn verdict with `--fail-on-warn`

### promote

Promotes a run receipt to become the new baseline.

**Required Arguments:**
- `--current`: Path to the run receipt to promote
- `--to`: Path where the baseline should be written

**Optional Arguments:**
- `--normalize`: Strip run-specific fields for stable baselines
- `--pretty`: Pretty-print JSON output

**Behavior:**
- Without `--normalize`, receipt MUST be copied unchanged
- With `--normalize`:
  - `run.id` MUST be replaced with "baseline"
  - `run.started_at` and `run.ended_at` MUST be replaced with "1970-01-01T00:00:00Z"
  - Host info, bench metadata, samples, and stats MUST be preserved

### export

Exports a run or compare receipt to CSV or JSONL format.

**Required Arguments (mutually exclusive):**
- `--run`: Path to run receipt
- `--compare`: Path to compare receipt

**Required Arguments:**
- `--out`: Output file path

**Optional Arguments:**
- `--format` (default: "csv"): Output format ("csv" or "jsonl")

**Behavior:**
- CSV output MUST be RFC 4180 compliant with header row
- JSONL output MUST have one JSON object per line
- Compare export MUST produce one row per metric
- Metrics MUST be sorted alphabetically for stable ordering
- Output MUST be deterministic (same input = same output)

**Run Export Columns:**
`bench_name, wall_ms_median, wall_ms_min, wall_ms_max, max_rss_kb_median, throughput_median, sample_count, timestamp`

**Compare Export Columns:**
`bench_name, metric, baseline_value, current_value, regression_pct, status, threshold`

### paired

Paired benchmarking with interleaved baseline/current runs to reduce environmental noise.

**Required Arguments:**
- `--baseline-cmd`: Baseline command to execute (shell string)
- `--current-cmd`: Current command to execute (shell string)

**Optional Arguments:**
- `--repeat` (default: 5): Number of paired samples
- `--warmup` (default: 0): Warmup pairs excluded from stats
- `--threshold` (default: 0.20): Regression threshold (fraction)
- `--warn-factor` (default: 0.90): Warn threshold = threshold * warn_factor
- `--fail-on-warn`: Treat warn verdict as exit 3
- `--out` (default: "perfgate-compare.json"): Output file path
- `--pretty`: Pretty-print JSON output

**Behavior:**
- MUST execute baseline and current commands alternately (B, C, B, C, ...)
- MUST measure each pair back-to-back to minimize environmental variance
- MUST compute statistics from non-warmup pairs only
- MUST compare using the same statistical methods as `compare`
- Output MUST conform to `perfgate.compare.v1` schema

**Exit Codes:**
- Exit 0: Pass verdict (or warn without `--fail-on-warn`)
- Exit 2: Fail verdict (budget violated)
- Exit 3: Warn verdict with `--fail-on-warn`

## Cockpit Mode

The `check` command supports `--mode cockpit` for integration with monitoring dashboards.

**Behavior:**
- Output `report.json` MUST conform to `sensor.report.v1` schema
- Extras artifacts MUST use versioned names: `perfgate.run.v1.json`, `perfgate.compare.v1.json`, `perfgate.report.v1.json`
- Error reports MUST use `tool.runtime` check_id with structured `{stage, error_kind}` data
- Baseline reason MUST use normalized `no_baseline` token
- Artifacts MUST be sorted by `(type, path)` for deterministic output

**Cockpit Mode Artifact Layout (single bench):**
```
artifacts/perfgate/
├── report.json                         # sensor.report.v1 envelope
├── comment.md
└── extras/
    ├── perfgate.run.v1.json
    ├── perfgate.compare.v1.json        (if baseline)
    └── perfgate.report.v1.json
```

**Cockpit Mode Artifact Layout (multi-bench `--all`):**
```
artifacts/perfgate/
├── report.json                         # aggregated sensor.report.v1
├── comment.md
└── extras/
    ├── bench-a/perfgate.run.v1.json
    ├── bench-a/perfgate.compare.v1.json
    ├── bench-a/perfgate.report.v1.json
    ├── bench-b/perfgate.run.v1.json
    └── ...
```

## Host Mismatch Detection

When comparing runs from different hosts, perfgate detects potential inconsistencies.

**Detection criteria:**
- Different `os` or `arch`
- Different `cpu_count`
- Different `hostname_hash` (if both present)

**Policy (`--host-mismatch`):**
- `warn` (default): Emit a warning but continue with comparison
- `error`: Exit 1 on mismatch
- `ignore`: Silently allow comparisons across different hosts

## CPU Time Tracking

On Unix platforms, perfgate collects CPU time metrics via `rusage`:
- `cpu_ms`: Combined user and system CPU time

These are optional fields in the run receipt sample. They are `None` on non-Unix platforms.

## Canonical Artifact Layout

The `check` command MUST write artifacts in the following structure:

```
artifacts/perfgate/
├── run.json        # perfgate.run.v1
├── compare.json    # perfgate.compare.v1 (if baseline exists)
├── report.json     # perfgate.report.v1 (always written by check)
└── comment.md      # PR comment markdown
```

## Exit Codes

All commands MUST use consistent exit codes:

| Code | Meaning | Description |
|------|---------|-------------|
| `0` | Success | Command completed; pass verdict; or warn without `--fail-on-warn`; or no baseline without `--require-baseline` |
| `1` | Tool error | I/O errors, parse failures, spawn failures, missing required arguments, command returned nonzero (without `--allow-nonzero`), baseline required but missing |
| `2` | Policy fail | Budget violated (regression exceeds threshold) |
| `3` | Warn as failure | Warn verdict with `--fail-on-warn` flag |

## Findings Model (Stable IDs)

Findings in `perfgate.report.v1` MUST use stable identifiers.

Baseline-missing finding:
- `check_id = "perf.baseline"`
- `code = "missing"`
- `severity = "warn"`

Budget findings:
- `check_id = "perf.budget"`
- `code = "metric_warn"` for warn metrics
- `code = "metric_fail"` for fail metrics

Truncation finding:
- `check_id = "tool.truncation"`
- `code = "truncated"`
- `severity = "info"`

### Finding Fingerprints

Every sensor report finding MUST include a `fingerprint` field containing the SHA-256 hex digest of a deterministic preimage.

| Finding type | Preimage | Example preimage |
|-------------|----------|-----------------|
| Metric budget | `{check_id}:{code}:{metric_name}` | `perf.budget:metric_fail:wall_ms` |
| Runtime error | `{check_id}:{code}:{stage}` | `tool.runtime:runtime_error:config_parse` |
| Truncation | `{check_id}:{code}` | `tool.truncation:truncated` |
| Multi-bench metric | `{bench_name}:{check_id}:{code}:{metric_name}` | `bench-a:perf.budget:metric_fail:wall_ms` |

### Finding Truncation

When `max_findings` is configured and the finding count exceeds the limit:
- The first `limit - 1` findings MUST be preserved
- A truncation meta-finding MUST be appended with `{total_findings, shown_findings}` data
- Total finding count after truncation MUST equal `limit`
- `verdict.reasons` MUST include `"truncated"` when truncation occurs
- `data` MUST include `findings_total` and `findings_emitted`

**Canonical definitions of truncation counters:**
- `findings_total`: count of real findings before truncation (excludes the truncation meta-finding itself)
- `findings_emitted`: count of real findings preserved after truncation (excludes the truncation meta-finding itself)
- Invariant when truncated: `findings.len() == findings_emitted + 1` (the +1 is the truncation meta-finding)
- When NOT truncated: both `findings_total` and `findings_emitted` are absent from `data`

## Baseline-Missing Behavior

When a baseline is not found:

| Flag | Behavior |
|------|----------|
| Neither flag | Warn to stderr, exit 0, write run receipt, report.json, and "no baseline" markdown; omit compare.json; include `no_baseline` reason token |
| `--require-baseline` | Exit 1 with error message |

Additional requirements:
- `report.json` MUST always be written by `check`
- For missing baseline without `--require-baseline`, `report.json` MUST have verdict status `warn` and `verdict.reasons` MUST include `no_baseline`
- For missing baseline without `--require-baseline`, `report.json` MUST include exactly one baseline-missing finding as specified above
- `compare.json` MUST be absent when baseline is missing, and stale compare artifacts MUST be removed if present

## Determinism Requirements

1. **Receipts**: Given identical execution results, receipts MUST be identical (excluding `run.id` and timestamps)

2. **Comparisons**: Given identical inputs, comparisons MUST produce identical output

3. **Reports**: Report generation MUST be deterministic (verified by property tests)

4. **Exports**: CSV and JSONL exports MUST be deterministic with stable ordering

5. **Rendering**: Markdown and annotation output MUST be stable

## Platform Notes

### RSS Collection

- `max_rss_kb` is collected via `rusage` from `wait4()` on Unix only
- On macOS, `ru_maxrss` is in bytes and MUST be converted to KB
- On Linux, `ru_maxrss` is in KB and MUST be used directly
- On non-Unix platforms, `max_rss_kb` MUST be `None`

### Timeout Behavior

- Timeout support requires Unix (uses `wait4` with `WNOHANG` polling)
- On Unix, timed-out commands are killed with `SIGKILL` and reaped
- On non-Unix platforms, timeout returns `AdapterError::TimeoutUnsupported`
- The `timed_out` flag MUST be set in the sample when timeout occurs

### Host Fingerprinting

- `os` and `arch` MUST be populated from `std::env::consts`
- `cpu_count` SHOULD be populated from `std::thread::available_parallelism`
- `memory_bytes` SHOULD be populated on Linux, macOS, and Windows
- `hostname_hash` is opt-in via `--include-hostname-hash` for privacy
