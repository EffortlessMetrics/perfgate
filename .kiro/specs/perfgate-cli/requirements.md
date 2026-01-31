# Requirements Document

## Introduction

perfgate is a Rust CLI tool for performance budgets and baseline diffs in CI/PR workflows. It runs benchmarks, emits versioned JSON receipts, compares results against baselines with configurable thresholds, and renders output for PR comments and GitHub Actions annotations.

## Glossary

- **CLI**: Command-line interface for perfgate
- **Receipt**: A versioned JSON document containing benchmark results or comparison outcomes
- **Run_Receipt**: JSON document (schema `perfgate.run.v1`) containing benchmark execution results
- **Compare_Receipt**: JSON document (schema `perfgate.compare.v1`) containing comparison results
- **Sample**: A single execution of the benchmark command with timing and resource metrics
- **Budget**: Threshold configuration defining acceptable regression limits for a metric
- **Metric**: A measurable value (wall_ms, max_rss_kb, throughput_per_s)
- **Verdict**: The overall pass/warn/fail status of a comparison
- **Baseline**: A reference Run_Receipt used for comparison
- **Process_Runner**: Component responsible for executing benchmark commands

## Requirements

### Requirement 1: Run Benchmark Command

**User Story:** As a developer, I want to run a benchmark command multiple times and collect timing metrics, so that I can establish performance baselines.

#### Acceptance Criteria

1. WHEN a user executes `perfgate run --name <name> -- <command>` THEN THE CLI SHALL execute the command and emit a Run_Receipt
2. WHEN the `--repeat` flag is provided THEN THE CLI SHALL execute the command that many times for measured samples (default: 5)
3. WHEN the `--warmup` flag is provided THEN THE CLI SHALL execute that many warmup iterations before measured samples (default: 0)
4. WHEN the `--work` flag is provided THEN THE CLI SHALL compute throughput_per_s as work_units / wall_seconds
5. WHEN the `--timeout` flag is provided THEN THE CLI SHALL terminate runs exceeding the duration and mark them as timed_out
6. WHEN the `--cwd` flag is provided THEN THE CLI SHALL execute the command in that working directory
7. WHEN the `--env` flag is provided THEN THE CLI SHALL set those environment variables for the command
8. WHEN the `--out` flag is provided THEN THE CLI SHALL write the Run_Receipt to that path (default: perfgate.json)
9. WHEN a command returns nonzero exit code THEN THE CLI SHALL record the exit_code in the sample
10. WHEN `--allow-nonzero` is not set and a command returns nonzero THEN THE CLI SHALL exit with code 1

### Requirement 2: Collect System Metrics

**User Story:** As a developer, I want to collect system resource metrics during benchmark runs, so that I can track memory usage alongside timing.

#### Acceptance Criteria

1. THE CLI SHALL always collect wall_ms (wall-clock time in milliseconds) for each sample
2. THE CLI SHALL always record exit_code for each sample
3. WHEN running on Unix THEN THE CLI SHALL collect max_rss_kb via wait4() rusage
4. WHEN running on non-Unix platforms THEN THE CLI SHALL omit max_rss_kb from samples
5. WHEN work_units is provided THEN THE CLI SHALL compute throughput_per_s for each sample

### Requirement 3: Compute Statistics

**User Story:** As a developer, I want summary statistics computed from my benchmark samples, so that I can understand typical performance.

#### Acceptance Criteria

1. THE CLI SHALL compute median, min, and max for wall_ms from non-warmup samples
2. WHEN max_rss_kb is available THEN THE CLI SHALL compute median, min, and max for max_rss_kb
3. WHEN throughput_per_s is available THEN THE CLI SHALL compute median, min, and max for throughput_per_s
4. THE CLI SHALL exclude warmup samples from statistics computation
5. IF no non-warmup samples exist THEN THE CLI SHALL return an error

### Requirement 4: Compare Receipts

**User Story:** As a developer, I want to compare current benchmark results against a baseline, so that I can detect performance regressions.

#### Acceptance Criteria

1. WHEN a user executes `perfgate compare --baseline <path> --current <path>` THEN THE CLI SHALL emit a Compare_Receipt
2. THE CLI SHALL compare median values for each metric present in both receipts
3. WHEN the `--threshold` flag is provided THEN THE CLI SHALL use that as the global regression threshold (default: 0.20)
4. WHEN the `--warn-factor` flag is provided THEN THE CLI SHALL compute warn_threshold as threshold * warn_factor (default: 0.90)
5. WHEN the `--metric-threshold` flag is provided THEN THE CLI SHALL override the threshold for that specific metric
6. WHEN the `--direction` flag is provided THEN THE CLI SHALL override the comparison direction for that metric
7. THE CLI SHALL use "lower is better" direction for wall_ms and max_rss_kb by default
8. THE CLI SHALL use "higher is better" direction for throughput_per_s by default

### Requirement 5: Determine Verdict

**User Story:** As a developer, I want a clear pass/warn/fail verdict from comparisons, so that I can gate CI pipelines on performance.

#### Acceptance Criteria

1. WHEN regression exceeds threshold THEN THE CLI SHALL set metric status to Fail
2. WHEN regression is between warn_threshold and threshold THEN THE CLI SHALL set metric status to Warn
3. WHEN regression is below warn_threshold THEN THE CLI SHALL set metric status to Pass
4. WHEN any metric has Fail status THEN THE CLI SHALL set verdict to Fail
5. WHEN no metrics have Fail but some have Warn THEN THE CLI SHALL set verdict to Warn
6. WHEN all metrics have Pass status THEN THE CLI SHALL set verdict to Pass
7. THE CLI SHALL include human-readable reasons for Warn and Fail verdicts

### Requirement 6: Exit Codes

**User Story:** As a CI engineer, I want predictable exit codes from perfgate, so that I can integrate it into automated pipelines.

#### Acceptance Criteria

1. WHEN the tool completes successfully with Pass verdict THEN THE CLI SHALL exit with code 0
2. WHEN a tool error occurs (I/O, parse, spawn failure) THEN THE CLI SHALL exit with code 1
3. WHEN the verdict is Fail THEN THE CLI SHALL exit with code 2
4. WHEN the verdict is Warn and `--fail-on-warn` is set THEN THE CLI SHALL exit with code 3
5. WHEN the verdict is Warn and `--fail-on-warn` is not set THEN THE CLI SHALL exit with code 0

### Requirement 7: Render Markdown

**User Story:** As a developer, I want to render comparison results as Markdown, so that I can post them as PR comments.

#### Acceptance Criteria

1. WHEN a user executes `perfgate md --compare <path>` THEN THE CLI SHALL render a Markdown table
2. THE Markdown_Renderer SHALL include a header showing the verdict status with emoji (✅/⚠️/❌)
3. THE Markdown_Renderer SHALL include the benchmark name
4. THE Markdown_Renderer SHALL render a table with columns: metric, baseline, current, delta, budget, status
5. WHEN there are verdict reasons THEN THE Markdown_Renderer SHALL include them as notes
6. WHEN `--out` is provided THEN THE CLI SHALL write to that file, otherwise print to stdout

### Requirement 8: GitHub Actions Annotations

**User Story:** As a CI engineer, I want GitHub Actions annotations for regressions, so that they appear inline in PR diffs.

#### Acceptance Criteria

1. WHEN a user executes `perfgate github-annotations --compare <path>` THEN THE CLI SHALL emit annotation lines
2. WHEN a metric has Fail status THEN THE CLI SHALL emit an `::error::` annotation
3. WHEN a metric has Warn status THEN THE CLI SHALL emit a `::warning::` annotation
4. WHEN a metric has Pass status THEN THE CLI SHALL NOT emit an annotation for it
5. THE annotation message SHALL include bench name, metric name, delta percentage, and baseline/current values

### Requirement 9: JSON Receipt Schemas

**User Story:** As a developer, I want versioned JSON schemas for receipts, so that I can validate and parse them reliably.

#### Acceptance Criteria

1. THE Run_Receipt SHALL include schema field with value "perfgate.run.v1"
2. THE Compare_Receipt SHALL include schema field with value "perfgate.compare.v1"
3. THE CLI SHALL support generating JSON Schema files via xtask
4. THE receipts SHALL use BTreeMap for deterministic key ordering
5. THE CLI SHALL perform atomic writes for receipt files to prevent corruption

### Requirement 10: Serialization Round-Trip

**User Story:** As a developer, I want receipts to serialize and deserialize correctly, so that I can store and retrieve them reliably.

#### Acceptance Criteria

1. FOR ALL valid Run_Receipt values, serializing to JSON then deserializing SHALL produce an equivalent value
2. FOR ALL valid Compare_Receipt values, serializing to JSON then deserializing SHALL produce an equivalent value
3. THE Parser SHALL handle malformed JSON gracefully without panicking
