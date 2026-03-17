# ADR 0006: Self-Dogfooding Performance Gating

## Status
Accepted

## Context
As `perfgate` grows in complexity, it is critical to ensure that its own performance does not regress and that its output artifacts remain stable. We need a system that can measure the tool's execution against fixed workloads and gate PRs based on these measurements.

## Decision
We will implement a self-dogfooding infrastructure with the following properties:

1.  **Multi-Lane Strategy**:
    *   **Smoke Lane**: Validates integration using the local GitHub Action.
    *   **Perf Lane**: Strictly gates performance of core commands against fixed fixtures.
    *   **Nightly Lane**: Performs high-precision calibration and trend analysis.
2.  **Authoritative Runner**: We pin `ubuntu-24.04` as the authoritative ruler. All canonical baselines are generated and evaluated in this environment.
3.  **Governed Baseline Lifecycle**: Baselines are refreshed via bot-generated Pull Requests from the nightly lane, ensuring an audit trail and preventing silent drift on `main`.
4.  **Split Configurations**: We maintain separate `.toml` configs for PR (speed) and Nightly (precision).
5.  **Deduplicated Wrappers**: Workload scripts are managed through a shared library (`lib.sh`) to ensure consistent exit classification and binary resolution.

## Consequences
*   PRs that introduce significant performance overhead in the core path will be blocked.
*   Baseline changes become visible in code review.
*   We gain long-term visibility into the performance characteristics of the tool.
*   CI runtime increases slightly due to binary compilation and repeated benchmark execution.
