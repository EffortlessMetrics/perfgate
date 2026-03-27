# Getting Started with perfgate on Bitbucket Pipelines

This guide explains how to integrate perfgate into your Bitbucket Pipelines.

## Prerequisites

1. A `perfgate.toml` config file in your repository (see [Configuration](CONFIG.md)).
2. Baselines stored in-repo (`baselines/` directory) or on a [baseline server](GETTING_STARTED_BASELINE_SERVER.md).

## Basic Setup

Add this to your `bitbucket-pipelines.yml`:

```yaml
image: rust:latest

pipelines:
  pull-requests:
    '**':
      - step:
          name: perfgate
          script:
            - cargo install perfgate-cli --locked
            - perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate || PERFGATE_EXIT=$?
            - exit ${PERFGATE_EXIT:-0}
          artifacts:
            - artifacts/perfgate/**
```

The wrapper `|| PERFGATE_EXIT=$?` captures a non-zero exit code so that
Bitbucket collects artifacts before the step fails. Exit code `2` means a
budget was violated. See [Caching](#caching) below for faster builds.

## Caching

Bitbucket supports custom cache definitions. Add a `cargo` cache to speed up
subsequent builds:

```yaml
definitions:
  caches:
    cargo: ~/.cargo

image: rust:latest

pipelines:
  pull-requests:
    '**':
      - step:
          name: perfgate
          caches:
            - cargo
          script:
            - cargo install perfgate-cli --locked
            - perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate || PERFGATE_EXIT=$?
            - exit ${PERFGATE_EXIT:-0}
          artifacts:
            - artifacts/perfgate/**
```

## With Baseline Server

If you use a centralized baseline server, set `PERFGATE_SERVER_URL` and
`PERFGATE_API_KEY` in **Repository settings > Pipelines > Repository variables**.
Bitbucket automatically injects repository variables into every step, so no
extra `export` lines are needed:

```yaml
image: rust:latest

pipelines:
  pull-requests:
    '**':
      - step:
          name: perfgate
          script:
            - cargo install perfgate-cli --locked
            - perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate || PERFGATE_EXIT=$?
            - exit ${PERFGATE_EXIT:-0}
          artifacts:
            - artifacts/perfgate/**
```

## Promoting Baselines After Merge

On your default branch, promote the current run to update baselines:

```yaml
image: rust:latest

pipelines:
  branches:
    main:
      - step:
          name: perfgate-promote
          script:
            - cargo install perfgate-cli --locked
            - perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate
            - perfgate promote --current artifacts/perfgate/run.json --to baselines/bench.json
          artifacts:
            - artifacts/perfgate/**
```

To commit the updated baseline back to the repository, add a git push step
after promotion or use the Bitbucket API to create a commit.

## Best Practices

- **Dedicated runners**: Use self-hosted runners with consistent hardware to minimize noise.
- **Paired mode**: For noisy environments, use `perfgate paired` instead of `perfgate check` for higher-confidence results.
- **Noise policy**: Set `noise_policy = "warn"` in `perfgate.toml` for inherently unstable benchmarks.
- **Artifacts**: Bitbucket does not upload artifacts from failed steps. Use the `|| PERFGATE_EXIT=$?` pattern shown above to defer the exit code so artifacts are collected before the step fails.
