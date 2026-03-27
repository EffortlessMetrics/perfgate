# Getting Started with perfgate on CircleCI

This guide explains how to integrate perfgate into your CircleCI pipelines.

## Prerequisites

1. A `perfgate.toml` config file in your repository (see [Configuration](CONFIG.md)).
2. Baselines stored in-repo (`baselines/` directory) or on a [baseline server](GETTING_STARTED_BASELINE_SERVER.md).

## Basic Setup

Add this to your `.circleci/config.yml`:

```yaml
version: 2.1

jobs:
  perfgate:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-{{ checksum "Cargo.lock" }}
            - cargo-
      - run:
          name: Install perfgate
          command: cargo install perfgate-cli --locked
      - save_cache:
          key: cargo-{{ checksum "Cargo.lock" }}
          paths:
            - ~/.cargo
      - run:
          name: Run perfgate checks
          command: perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate
      - store_artifacts:
          path: artifacts/perfgate
          destination: perfgate
          when: always

workflows:
  pr-check:
    jobs:
      - perfgate
```

Exit code `2` fails the job when a budget is violated.

## With Baseline Server

If you use a centralized baseline server, set `PERFGATE_SERVER_URL` and
`PERFGATE_API_KEY` in **Project Settings > Environment Variables**. CircleCI
automatically exposes project-level environment variables to every job, so no
extra configuration is needed in the config file -- the basic setup above works
as-is.

## Promoting Baselines After Merge

Use a workflow filter to run promotion only on the main branch:

```yaml
version: 2.1

jobs:
  perfgate:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-{{ checksum "Cargo.lock" }}
            - cargo-
      - run:
          name: Install perfgate
          command: cargo install perfgate-cli --locked
      - save_cache:
          key: cargo-{{ checksum "Cargo.lock" }}
          paths:
            - ~/.cargo
      - run:
          name: Run perfgate checks
          command: perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate
      - store_artifacts:
          path: artifacts/perfgate
          destination: perfgate
          when: always

  perfgate-promote:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - restore_cache:
          keys:
            - cargo-{{ checksum "Cargo.lock" }}
            - cargo-
      - run:
          name: Install perfgate
          command: cargo install perfgate-cli --locked
      - save_cache:
          key: cargo-{{ checksum "Cargo.lock" }}
          paths:
            - ~/.cargo
      - run:
          name: Run and promote baselines
          command: |
            perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate
            perfgate promote --current artifacts/perfgate/run.json --to baselines/bench.json
      - store_artifacts:
          path: artifacts/perfgate
          destination: perfgate
          when: always

workflows:
  pr-check:
    jobs:
      - perfgate:
          filters:
            branches:
              ignore: main
  promote:
    jobs:
      - perfgate-promote:
          filters:
            branches:
              only: main
```

## Caching

The examples above use `restore_cache`/`save_cache` keyed on `Cargo.lock` to
avoid reinstalling perfgate on every run. If your project does not have a
`Cargo.lock` checked in, use a static key:

```yaml
- restore_cache:
    keys:
      - cargo-perfgate-v1
- save_cache:
    key: cargo-perfgate-v1
    paths:
      - ~/.cargo
```

## Common Pitfalls

**Warning: perfgate exits with code 2 on budget violations.** CircleCI treats any
non-zero exit code as a step failure. Commands after the failing line in the same
`run` block will not execute unless you capture the exit code:

```yaml
      - run:
          name: Run perfgate checks
          command: |
            perfgate check --config perfgate.toml --all || EXIT=$?
            # ... post-processing here ...
            exit ${EXIT:-0}
```

**Warning: `store_artifacts` defaults to `on_success`.** Without `when: always`,
CircleCI only collects artifacts from successful steps. Since perfgate uses exit
code 2 for policy failures, artifacts from failed runs would be silently lost:

```yaml
      - store_artifacts:
          path: artifacts/perfgate
          destination: perfgate
          when: always            # <-- critical
```

**Warning: the `environment` key uses literal strings, not shell interpolation.**
Do *not* use `${VAR}` syntax in the `environment` block -- it will be treated as
a literal string, not expanded. Instead, set sensitive values as project environment
variables in **Project Settings > Environment Variables** and reference them directly
in shell commands. For example, this is **wrong**:

```yaml
    # WRONG -- ${PERFGATE_API_KEY} is a literal string here
    environment:
      PERFGATE_API_KEY: "${PERFGATE_API_KEY}"
```

And this is **correct** -- just set `PERFGATE_API_KEY` as a project env var and it
will be available automatically, no `environment` block needed.

**Warning: understand the exit code semantics.** perfgate uses three distinct
non-zero exit codes:
- **1** -- tool/runtime error (I/O failure, parse error, spawn failure)
- **2** -- policy fail (budget violated)
- **3** -- warn treated as failure (`--fail-on-warn`)

All three cause step failure in CircleCI unless captured.

## Best Practices

- **Resource classes**: Use a dedicated resource class with consistent hardware to minimize noise.
- **Paired mode**: For noisy environments, use `perfgate paired` instead of `perfgate check` for higher-confidence results.
- **Noise policy**: Set `noise_policy = "warn"` in `perfgate.toml` for inherently unstable benchmarks.
- **Artifacts**: Always use `store_artifacts` so results are available even when the job fails.
