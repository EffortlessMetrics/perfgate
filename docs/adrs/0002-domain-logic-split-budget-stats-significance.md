# ADR 0002: Domain Logic Split (Budget, Stats, Significance)

## Status
Accepted

## Context
The `perfgate-domain` crate originally handled everything from raw sample storage to statistical analysis and budget policy enforcement. This made it hard to reason about the "pure" parts of the system versus the policy-driven parts.

## Decision
We extracted the core mathematical and policy logic into specialized crates:
- `perfgate-stats`: Provides `U64Summary` and `F64Summary` for pure statistical aggregation.
- `perfgate-budget`: Implements the logic for comparing metrics against thresholds and determining `Pass/Warn/Fail` status.
- `perfgate-significance`: Contains the statistical significance logic (Welch's t-test and p-values).

`perfgate-domain` now acts as a coordinator for these domain entities, focusing on the high-level measurement models.

## Consequences
- Statistical logic is now reusable in contexts where budget policy is not needed.
- Budget policy can be tested independently of how the statistics were gathered.
- Significance testing remains an optional, high-value component that doesn't bloat the core stats logic.
