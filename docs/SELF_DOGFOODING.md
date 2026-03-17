# Self-Dogfooding System

Perfgate uses itself to monitor and gate its own performance. This ensures that the tool remains efficient and that its output artifacts stay stable across releases.

## Multi-Lane CI Strategy

We use three distinct lanes to validate different aspects of the system:

| Lane | Purpose | Trigger | authoritative? |
|------|---------|---------|----------------|
| **Action Smoke Lane** | Validates the GitHub Action integration, installation path, and user-facing ergonomics. | PR, Push | No |
| **Core Perf Lane** | Strictly gates the performance of core commands (`compare`, `check`, `md`, `report`) against fixed workloads. | PR, Push | **Yes** |
| **Nightly Calibration** | Observes long-term trends, runs heavier benchmarks, and refreshes canonical baselines via bot PRs. | Nightly | **Yes** |

## Authoritative Environment

Performance metrics are sensitive to the environment. We pin our authoritative runner to:

- **OS/Image**: `ubuntu-24.04` (fixed version)
- **Arch**: `x86_64`
- **Namespace**: `baselines/gha-ubuntu-24.04-x86_64/`

Metrics from other platforms (e.g., Windows) are currently observe-only.

## Automation (xtask)

We use `xtask` to manage dogfooding fixtures and verification:

- `cargo run -p xtask -- dogfood fixtures`: Regenerates the stable JSON fixtures in `.ci/fixtures/` using the release binary.
- `cargo run -p xtask -- dogfood verify`: Validates that required artifacts (`report.json`, `comment.md`) exist in the expected layout.

## Configurations

- **`.ci/perfgate-pr.toml`**: Optimized for speed. Runs fewer repetitions and focuses on the most stable critical path benchmarks.
- **`.ci/perfgate-nightly.toml`**: Optimized for precision. Runs 15+ repetitions, enables statistical significance testing (Welch's t-test), and generates trend data.

## Artifacts

Artifacts are produced in `artifacts/perfgate/` and uploaded with unique names per run attempt to avoid immutability conflicts:
`perfgate-artifacts-{lane}-{run_id}-{run_attempt}`

## Bootstrapping

If a new benchmark is added without a baseline:
1. CI will emit a warning `no baseline found`.
2. The benchmark will default to a `pass` state.
3. The next Nightly run will detect the missing baseline, generate it, and propose it via a "Baseline Refresh" PR.
