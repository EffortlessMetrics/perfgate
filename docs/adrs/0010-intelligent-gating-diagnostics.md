# ADR 0010: Intelligent Gating Diagnostics (Bisect, Blame, Explain)

## Status
Accepted

## Context
When a performance regression is detected, the natural next question is "why?" Traditionally this requires manual investigation: reviewing recent commits, checking dependency changes, and correlating diffs with performance deltas. This is slow and error-prone, especially for large PRs or dependency-heavy projects.

## Decision
v0.15.0 introduces three diagnostic commands that automate common investigation workflows:

### `perfgate bisect`
Wraps `git bisect` with `perfgate paired` to automatically find the commit that introduced a regression. Takes a known-good commit, a known-bad commit (default: HEAD), and an executable to benchmark.

### `perfgate blame`
Analyzes two `Cargo.lock` files (baseline vs current) to identify dependency additions, removals, and version changes. Useful for binary size regressions caused by transitive dependency bloat.

### `perfgate explain`
Generates structured diagnostic prompts from a comparison receipt. Designed to be piped into an LLM or used as a human-readable regression report. Does not call any external AI service — it produces the prompt, not the answer.

Key design choices:
1. **Each command is standalone** — they don't require the full `check` pipeline. You can run `blame` without having run `compare`.
2. **No external dependencies** — `bisect` uses local git, `blame` parses Cargo.lock, `explain` generates text. No API calls, no network.
3. **Output is text, not JSON** — these are diagnostic tools, not pipeline stages. They write human-readable output to stdout.

## Consequences
- Investigation time for regressions drops from hours to minutes.
- `bisect` accuracy depends on the benchmark being deterministic enough to distinguish good from bad — noisy benchmarks may give false results.
- `explain` output quality depends on the comparison having enough context (metrics, deltas, significance) to be useful.
- These commands are newer and less battle-tested than the core pipeline.
