# perfgate Design

This document describes the measurement model, regression mathematics, and algorithmic design of perfgate.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## Measurement Model

### Samples and Statistics

perfgate collects raw samples and computes summary statistics:

**Sample Fields:**
- `wall_ms`: Wall-clock execution time in milliseconds
- `exit_code`: Process exit code
- `warmup`: Boolean flag indicating warmup sample
- `timed_out`: Boolean flag indicating timeout occurred
- `max_rss_kb`: Maximum resident set size in KB (Unix only, optional)
- `stdout`: Truncated stdout (optional, up to `output_cap_bytes`)
- `stderr`: Truncated stderr (optional, up to `output_cap_bytes`)

**Statistics (U64Summary):**
- `median`: Median value (middle element for odd count, floor of average of two middle elements for even count)
- `min`: Minimum value
- `max`: Maximum value

**Statistics (F64Summary):**
- `median`: Median value (average of two middle elements for even count)
- `min`: Minimum value
- `max`: Maximum value

### Warmup Semantics

Warmup samples are executed before measured samples to prime caches, JIT compilers, and other runtime systems.

**Invariants:**
1. Total iterations = `warmup + repeat`
2. First `warmup` iterations MUST have `sample.warmup = true`
3. Remaining `repeat` iterations MUST have `sample.warmup = false`
4. Statistics MUST be computed from non-warmup samples only
5. Adding or modifying warmup samples MUST NOT change computed statistics

### Repeat Count

The `repeat` parameter controls the number of measured samples:

- MUST be at least 1
- Default is 5 (provides reasonable median stability)
- Higher values increase measurement confidence but extend execution time

### Throughput Calculation

When `work_units` is specified, throughput is computed as:

```
throughput_per_s = work_units / (wall_ms / 1000.0)
```

**Edge Cases:**
- If `wall_ms = 0`, throughput is `0.0` (not infinity)
- Throughput is computed per-sample, then summarized

## Regression Mathematics

### Baseline vs Current Comparison

For each metric with a configured budget, perfgate computes:

```rust
let ratio = current / baseline;        // e.g., 1.10 for 10% increase
let pct = (current - baseline) / baseline;  // e.g., 0.10 for 10% increase
```

### Direction Semantics

Each metric has a **direction** that determines what constitutes a regression:

| Direction | Meaning | Regression When |
|-----------|---------|-----------------|
| `Lower` | Lower values are better | `current > baseline` (positive pct) |
| `Higher` | Higher values are better | `current < baseline` (negative pct) |

**Default Directions:**
- `wall_ms`: Lower (faster is better)
- `max_rss_kb`: Lower (less memory is better)
- `throughput_per_s`: Higher (more work per second is better)

### Regression Calculation

The `regression` value is the normalized positive regression amount:

```rust
let regression = match direction {
    Direction::Lower => pct.max(0.0),     // Positive when current > baseline
    Direction::Higher => (-pct).max(0.0), // Positive when current < baseline
};
```

**Invariant:** `regression >= 0.0` always.

## Budget Thresholds

### Threshold Configuration

Each budget has two thresholds:

- `threshold`: The fail threshold (e.g., 0.20 for 20%)
- `warn_threshold`: The warn threshold (e.g., 0.18 for 18%)

**Relationship:**
```rust
warn_threshold = threshold * warn_factor
```

Where `warn_factor` defaults to 0.90.

### Status Determination

Metric status is determined by comparing regression to thresholds:

```rust
let status = if regression > threshold {
    MetricStatus::Fail
} else if regression >= warn_threshold {
    MetricStatus::Warn
} else {
    MetricStatus::Pass
};
```

**Boundary Conditions:**
- `regression > threshold` -> Fail
- `warn_threshold <= regression <= threshold` -> Warn
- `regression < warn_threshold` -> Pass

### Verdict Aggregation

The overall verdict is determined by the worst metric status:

```rust
let status = if counts.fail > 0 {
    VerdictStatus::Fail
} else if counts.warn > 0 {
    VerdictStatus::Warn
} else {
    VerdictStatus::Pass
};
```

### Verdict Reasons (Stable Tokens)

`verdict.reasons` stores stable tokens for downstream tooling. Tokens are not prose.

Format:
```
<metric>_<warn|fail>
```

Examples:
- `wall_ms_warn`
- `max_rss_kb_fail`
- `throughput_per_s_warn`

When no baseline is available, `check` uses the token `no_baseline`.

## Delta Structure

Each compared metric produces a Delta record:

```rust
pub struct Delta {
    pub baseline: f64,    // Baseline median value
    pub current: f64,     // Current median value
    pub ratio: f64,       // current / baseline
    pub pct: f64,         // (current - baseline) / baseline
    pub regression: f64,  // Positive regression amount (0 if improvement)
    pub status: MetricStatus,  // Pass, Warn, or Fail
}
```

## Report Synthesis

### Finding Generation

The report command generates findings for metrics with Warn or Fail status:

1. Iterate over deltas in `BTreeMap` order (sorted by metric enum)
2. For each delta with status != Pass, create a `ReportFinding`
3. Set finding code: "metric_warn" or "metric_fail"
4. Set check_id: always "perf.budget"
5. Populate `FindingData` with metric details

**Invariants:**
- Finding count MUST equal warn count + fail count
- Findings MUST be ordered deterministically by metric name
- Report verdict MUST match compare verdict

### Stable Ordering

perfgate uses `BTreeMap` for all metric collections to ensure deterministic ordering:

- `Metric` enum implements `Ord` with ordering: `WallMs < MaxRssKb < ThroughputPerS`
- This ordering is preserved in JSON serialization (snake_case: `max_rss_kb < throughput_per_s < wall_ms`)
- Export commands sort metrics alphabetically for user-friendliness

## Promote Normalization

The promote command can normalize receipts for stable baselines:

### Without Normalization

The receipt is copied unchanged. This preserves:
- Original `run.id` (UUID)
- Original timestamps
- All other fields

### With Normalization (`--normalize`)

Run-specific fields are replaced with stable values:

| Field | Original | Normalized |
|-------|----------|------------|
| `run.id` | UUID | "baseline" |
| `run.started_at` | RFC 3339 timestamp | "1970-01-01T00:00:00Z" |
| `run.ended_at` | RFC 3339 timestamp | "1970-01-01T00:00:00Z" |

**Preserved Fields:**
- `run.host` (all fields including cpu_count, memory_bytes, hostname_hash)
- `bench` (all metadata)
- `samples` (all sample data)
- `stats` (all computed statistics)
- `schema`, `tool`

## Export Formats

### CSV Format

RFC 4180 compliant CSV with the following characteristics:

- Header row MUST be present
- Fields containing comma, double quote, or newline MUST be quoted
- Double quotes within quoted fields MUST be escaped as `""`
- Rows end with `\n`

**Run Export Column Order:**
1. bench_name
2. wall_ms_median
3. wall_ms_min
4. wall_ms_max
5. max_rss_kb_median (empty if None)
6. throughput_median (empty if None, 6 decimal places if present)
7. sample_count
8. timestamp

**Compare Export Column Order:**
1. bench_name
2. metric
3. baseline_value (6 decimal places)
4. current_value (6 decimal places)
5. regression_pct (6 decimal places, percentage e.g., 10.5 for 10.5%)
6. status (lowercase: pass/warn/fail)
7. threshold (6 decimal places, percentage)

### JSONL Format

JSON Lines format with one JSON object per line:

- Each line MUST be a complete, valid JSON object
- Lines end with `\n`
- No trailing comma after the last field
- Field order matches CSV column order

## Overflow-Safe Median

The median algorithm for `u64` values uses an overflow-safe formula:

```rust
// For even-length sorted arrays with middle elements a and b:
let median = (a / 2) + (b / 2) + ((a % 2 + b % 2) / 2);
```

This avoids overflow when `a + b > u64::MAX` by:
1. Computing half of each value first
2. Adding the remainder halves separately
3. Rounding down (floor division)

**Verification:**
Property tests verify correctness against a reference implementation using `u128`.

## Metric Value Extraction

Statistics use median values for comparison:

```rust
fn metric_value(stats: &Stats, metric: Metric) -> Option<f64> {
    match metric {
        Metric::WallMs => Some(stats.wall_ms.median as f64),
        Metric::MaxRssKb => stats.max_rss_kb.as_ref().map(|s| s.median as f64),
        Metric::ThroughputPerS => stats.throughput_per_s.as_ref().map(|s| s.median),
    }
}
```

**Invariants:**
- `wall_ms` is always present (never None)
- `max_rss_kb` MAY be None (non-Unix or collection failure)
- `throughput_per_s` MAY be None (no `work_units` specified)
- Metrics missing from either baseline or current are skipped in comparison
