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

workflows:
  pr-check:
    jobs:
      - perfgate
```

Exit code `2` fails the job when a budget is violated.

## With Baseline Server

If you use a centralized baseline server, pass credentials via project environment variables:

```yaml
jobs:
  perfgate:
    docker:
      - image: rust:latest
    environment:
      PERFGATE_SERVER_URL: ${PERFGATE_SERVER_URL}
      PERFGATE_API_KEY: ${PERFGATE_API_KEY}
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
```

Set `PERFGATE_SERVER_URL` and `PERFGATE_API_KEY` in **Project Settings > Environment Variables**.

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

## Best Practices

- **Resource classes**: Use a dedicated resource class with consistent hardware to minimize noise.
- **Paired mode**: For noisy environments, use `perfgate paired` instead of `perfgate check` for higher-confidence results.
- **Noise policy**: Set `noise_policy = "warn"` in `perfgate.toml` for inherently unstable benchmarks.
- **Artifacts**: Always use `store_artifacts` so results are available even when the job fails.
