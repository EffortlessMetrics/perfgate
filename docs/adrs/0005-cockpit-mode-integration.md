# ADR 0005: Cockpit Mode Integration

## Status

Accepted

## Context

Monitoring performance in CI/CD environments often requires integration with external dashboards, cockpit-style views, or automated PR commenting systems. These consumers need:

1.  **Stable Data Contract**: A predictable JSON envelope that remains stable even as internal tool implementation details change.
2.  **Predictable Exit Codes**: In many monitoring scenarios, a "performance failure" (budget violation) should not necessarily break the build pipeline, but should still be recorded and reported to a dashboard.
3.  **Rich Artifacts**: Access to raw data (run receipts, comparisons) alongside the high-level summary.

## Decision

We introduced "Cockpit Mode" (`--mode cockpit`) for the `check` command to provide a standardized interface for monitoring integrations.

### Key Features

1.  **`sensor.report.v1` Envelope**: All output is wrapped in a standardized sensor report schema (vendored in `contracts/schemas/`). This envelope contains the verdict, summary statistics, and structured findings.
2.  **Stable Exit Code Behavior**: In cockpit mode, `perfgate check` always exits 0 unless a catastrophic failure occurs (e.g., unable to write the report). Budget violations are captured in the JSON report rather than signaled via exit code 2.
3.  **Versioned Extra Artifacts**: All detailed artifacts are placed in an `extras/` subdirectory with versioned filenames (e.g., `perfgate.run.v1.json`), ensuring long-term compatibility for tools that deep-link into the raw data.
4.  **Deterministic Fingerprinting**: Every finding in the report includes a SHA-256 fingerprint, allowing dashboards to track the same finding across multiple runs and suppress duplicates.
5.  **Multi-bench Support**: Cockpit mode supports aggregated reports when running multiple benchmarks via `--all`, providing a single `report.json` for the entire suite.

## Consequences

### Positive

*   **First-class Dashboard Integration**: Provides a "plug-and-play" experience for performance monitoring platforms.
*   **Resilient CI Pipelines**: Prevents performance regressions from blocking critical path CI while still ensuring they are visible.
*   **Schema Isolation**: Separates the internal tool-specific schemas from the external integration contract.

### Negative

*   **Double-wrapping**: Adds a layer of indirection to the JSON output.
*   **Artifact Proliferation**: Generates more files in the `artifacts/` directory compared to standard mode.
