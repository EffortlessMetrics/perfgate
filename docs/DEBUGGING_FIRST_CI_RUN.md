# Debugging the First CI Run

This guide covers the first run after `perfgate init --ci github --profile standard`.

## Start Locally

Run the same config locally before reading CI logs:

```bash
perfgate check --config perfgate.toml --all
perfgate baseline status --config perfgate.toml
```

If the check created a trusted first run, promote it into local baselines:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Commit the generated baselines before expecting the generated GitHub workflow to
pass with `require_baseline: "true"`.

## Missing Baseline

The generated workflow requires baselines by default. That is intentional: a CI
gate should not silently pass without a comparison point.

If CI reports a missing baseline:

```bash
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

Then commit `baselines/` and rerun CI.

## Artifact Paths

The generated config writes artifacts under `artifacts/perfgate`.

For `check --bench <name>`:

```text
artifacts/perfgate/
  run.json
  compare.json  # when a baseline exists
  report.json
  comment.md
```

For `check --all`, even when the config only has one benchmark:

```text
artifacts/perfgate/<bench>/
  run.json
  compare.json  # when a baseline exists
  report.json
  comment.md
```

If no baseline exists yet, inspect `run.json`, `report.json`, and `comment.md`.
`compare.json` is written only when there is a baseline to compare against.

## Exit Codes

`perfgate` uses exit codes to separate tool failures from performance policy:

- `1`: tool or runtime error, such as config, I/O, parse, or command failure
- `2`: budget policy failed
- `3`: warning was treated as failure with `--fail-on-warn`

For setup failures, run:

```bash
perfgate doctor --config perfgate.toml
```

For budget failures, reproduce the specific benchmark locally:

```bash
perfgate check --config perfgate.toml --bench parser
```

Then inspect the matching artifact directory.

## Next References

- [GitHub Actions](GETTING_STARTED_GITHUB_ACTIONS.md)
- [Artifact Layouts](ARTIFACTS.md)
- [Configuration](CONFIG.md)
- [Failure Playbook](FAILURE_PLAYBOOK.md)
