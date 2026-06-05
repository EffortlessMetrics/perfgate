# Getting Started with perfgate on GitLab CI

This guide explains how to integrate perfgate into your GitLab CI/CD pipelines.

## Prerequisites

1. A `perfgate.toml` config file in your repository (see [Configuration](CONFIG.md)).
2. Baselines stored in-repo (`baselines/` directory) or on a [baseline server](GETTING_STARTED_BASELINE_SERVER.md).

## Basic Setup

Add this to your `.gitlab-ci.yml`:

```yaml
perfgate:
  image: rust:1.95.0
  stage: test
  before_script:
    - cargo install perfgate-cli --locked --version 0.18.0
  script:
    - perfgate check --config perfgate.toml --all
  artifacts:
    when: always
    paths:
      - artifacts/perfgate/
```

Exit code `2` fails the job when a budget is violated.

## With Baseline Server

If you use a centralized baseline server, pass credentials via CI/CD variables:

```yaml
perfgate:
  image: rust:1.95.0
  stage: test
  variables:
    PERFGATE_SERVER_URL: $PERFGATE_SERVER_URL
    PERFGATE_API_KEY: $PERFGATE_API_KEY
  before_script:
    - cargo install perfgate-cli --locked --version 0.18.0
  script:
    - perfgate check --config perfgate.toml --all
  artifacts:
    when: always
    paths:
      - artifacts/perfgate/
```

Set `PERFGATE_SERVER_URL` and `PERFGATE_API_KEY` in **Settings > CI/CD > Variables**.

## Promoting Baselines After Merge

On your default branch, promote only a reviewed, representative current run to
update baselines:

```yaml
perfgate-promote:
  image: rust:1.95.0
  stage: deploy
  only:
    - main
  before_script:
    - cargo install perfgate-cli --locked --version 0.18.0
  script:
    - perfgate check --config perfgate.toml --all
    - '# Review artifacts and confirm the command is representative before promotion.'
    - perfgate baseline promote --config perfgate.toml --all
  artifacts:
    paths:
      - baselines/
```

The install pin should match the perfgate release you intend to run. The
promotion step uses the config-aware baseline command so `check --all`
per-benchmark artifacts are promoted from their configured locations.

## Common Pitfalls

**Warning: perfgate exits with code 2 on budget violations.** This is intentional
(exit code 2 = policy fail), but it means the `script` phase fails and the job is
marked as failed. Any commands listed after the perfgate invocation in `script:` will
*not* run. If you need post-processing (e.g., promoting baselines) after a potential
failure, use `after_script:` or split into separate jobs.

**Warning: `when: always` is required on the `artifacts:` block.** Without it,
GitLab only uploads artifacts on success. Since perfgate signals budget violations
via exit code 2, artifacts from failed runs would be silently lost:

```yaml
  artifacts:
    when: always          # <-- critical
    paths:
      - artifacts/perfgate/
```

The basic setup in this guide already includes `when: always`, but make sure any
custom job definitions do the same.

**Warning: understand the exit code semantics.** perfgate uses three distinct
non-zero exit codes:
- **1** -- tool/runtime error (I/O failure, parse error, spawn failure)
- **2** -- policy fail (budget violated)
- **3** -- warn treated as failure (`--fail-on-warn`)

GitLab CI treats any non-zero exit as a job failure. If you need to capture the exit
code and continue, use `allow_failure: true` or wrap the command:

```yaml
  script:
    - perfgate check --config perfgate.toml --all || PERFGATE_EXIT=$?
    - echo "perfgate exited with ${PERFGATE_EXIT:-0}"
    - exit ${PERFGATE_EXIT:-0}
```

## Best Practices

- **Tagged runners**: Run performance checks on dedicated runners with consistent hardware to minimize noise.
- **Paired mode**: For noisy environments, use `perfgate paired` instead of `perfgate check` for higher-confidence results.
- **Noise policy**: Set `noise_policy = "warn"` in `perfgate.toml` for inherently unstable benchmarks.
