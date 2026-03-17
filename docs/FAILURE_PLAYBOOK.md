# Performance Gating Failure Playbook

This guide helps you resolve failures in the `perfgate-self` or `perfgate-nightly` workflows.

## 1. Runtime Failure (Non-zero exit != 2, 3)

The workload wrapper script failed before it could evaluate the performance policy.

- **Check Logs**: Look for "perfgate binary not found" or shell script errors.
- **Cause**: Usually due to changes in the project structure, broken build, or missing dependencies in the runner image.
- **Fix**: Verify `.ci/perf/lib.sh` and the individual wrapper scripts.

## 2. Policy Failure (Exit Code 2 or 3)

The benchmark ran successfully, but the performance exceeded the budget.

- **Inspect Artifacts**: Download the `perfgate-artifacts-core-...` artifact and check `comment.md` or `report.json`.
- **Verify Regression**: If the regression is intentional (e.g., adding a new feature), wait for the Nightly lane to propose a new baseline, or manually promote your run.
- **Fix Noise**: If the failure is due to runner noise, retry the job or investigate if the benchmark is flakier than usual.

## 3. Artifact Conflict (409 Conflict)

GitHub Action failed to upload an artifact because the name already exists.

- **Cause**: Retrying a job within the same workflow run, or two jobs trying to upload to the same name.
- **Fix**: We use `${{ github.run_id }}-${{ github.run_attempt }}` in artifact names to prevent this. Ensure your workflow file is updated to the latest version from `main`.

## 4. No Baseline Found

The CI emits a warning instead of a failure.

- **Cause**: A new benchmark was added to the `.toml` config but no JSON fixture exists in `baselines/`.
- **Fix**: Let the Nightly run generate it, or run `perfgate check` locally and commit the promoted receipt to the correct namespace.

## 5. Host Mismatch Warning

Warning: `host mismatch: OS mismatch: baseline=linux, current=windows`.

- **Cause**: Comparing runs from different environments.
- **Fix**: Ensure you are only promoting authoritative baselines from the `ubuntu-24.04` runner.
