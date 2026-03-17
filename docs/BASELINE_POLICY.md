# Performance Baseline Policy

This document defines how we manage and govern performance baselines for the `perfgate` project.

## 1. Authoritative Source

The **GitHub Actions `ubuntu-24.04` x86_64** runner is the authoritative environment for all canonical performance metrics.

- Baselines are stored in: `baselines/gha-ubuntu-24.04-x86_64/`
- Host/OS mismatches are not allowed for authoritative gating.

## 2. Refresh Lifecycle

Baselines are refreshed exclusively through the **Nightly Calibration Lane**.

1. Nightly run measures the current `main` branch with high precision (15+ samples).
2. Candidate baselines are generated.
3. A **Baseline Refresh PR** is automatically opened by the `github-actions[bot]`.
4. The PR is allowed to **auto-merge** if all CI checks pass.

## 3. Threshold Governance

Unlike baselines, **performance thresholds are manual-only**.

- Automatic loosening of thresholds is strictly forbidden.
- Tightening thresholds should be done when a benchmark shows zero false positives over 30+ runs and observed variance is significantly lower than the budget.
- Threshold changes must be justified in the PR description.

## 4. Multi-Platform Support

Windows metrics are tracked but currently kept in a separate, non-authoritative namespace. They are observe-only and do not gate PR merges unless explicitly enabled for a specific platform-critical benchmark.
