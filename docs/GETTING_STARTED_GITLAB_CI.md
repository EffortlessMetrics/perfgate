# Getting Started with perfgate on GitLab CI

This guide explains how to integrate perfgate into your GitLab CI/CD pipelines.

## Prerequisites

1. A `perfgate.toml` config file in your repository (see [Configuration](CONFIG.md)).
2. Baselines stored in-repo (`baselines/` directory) or on a [baseline server](GETTING_STARTED_BASELINE_SERVER.md).

## Basic Setup

Add this to your `.gitlab-ci.yml`:

```yaml
perfgate:
  image: rust:latest
  stage: test
  before_script:
    - cargo install --path crates/perfgate-cli
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
  image: rust:latest
  stage: test
  variables:
    PERFGATE_SERVER_URL: $PERFGATE_SERVER_URL
    PERFGATE_API_KEY: $PERFGATE_API_KEY
  before_script:
    - cargo install --path crates/perfgate-cli
  script:
    - perfgate check --config perfgate.toml --all
  artifacts:
    when: always
    paths:
      - artifacts/perfgate/
```

Set `PERFGATE_SERVER_URL` and `PERFGATE_API_KEY` in **Settings > CI/CD > Variables**.

## Promoting Baselines After Merge

On your default branch, promote the current run to update baselines:

```yaml
perfgate-promote:
  image: rust:latest
  stage: deploy
  only:
    - main
  before_script:
    - cargo install --path crates/perfgate-cli
  script:
    - perfgate check --config perfgate.toml --all
    - perfgate promote --current artifacts/perfgate/run.json --to baselines/bench.json
  artifacts:
    paths:
      - baselines/
```

## Best Practices

- **Tagged runners**: Run performance checks on dedicated runners with consistent hardware to minimize noise.
- **Paired mode**: For noisy environments, use `perfgate paired` instead of `perfgate check` for higher-confidence results.
- **Noise policy**: Set `noise_policy = "warn"` in `perfgate.toml` for inherently unstable benchmarks.
