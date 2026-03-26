# Getting Started with perfgate on GitLab CI

This guide explains how to integrate `perfgate` into your GitLab CI/CD pipelines for automated performance gating.

## Prerequisites

1.  A `perfgate` baseline server (see [Getting Started with Baseline Server](./GETTING_STARTED_BASELINE_SERVER.md)).
2.  An API Key with `contributor` scope for your project.
3.  A `perfgate.toml` file in your repository root.

## Integration using Template

The easiest way to integrate is using the official GitLab CI template.

Add the following to your `.gitlab-ci.yml`:

```yaml
include:
  - remote: 'https://raw.githubusercontent.com/effortlesssteven/perfgate/main/gitlab-ci/perfgate.yml'

performance-check:
  extends: .perfgate-check
  variables:
    PERFGATE_PROJECT: "your-project-id"
    PERFGATE_SERVER_URL: "https://your-perfgate-server.com"
    PERFGATE_API_KEY: $PERFGATE_API_KEY # Set this in GitLab CI/CD Variables
```

## Manual Integration

If you prefer manual control, you can define the job yourself:

```yaml
perfgate-check:
  image: rust:latest
  stage: test
  script:
    - cargo install perfgate --version v0.11.0
    - mkdir -p artifacts/perfgate
    - perfgate check --project "my-project" --server $PERFGATE_URL --api-key $PERFGATE_KEY --output-md artifacts/perfgate/report.md
  artifacts:
    when: always
    paths:
      - artifacts/perfgate/
    expose_as: 'Performance Report'
```

## Viewing Results

After the pipeline runs:

1.  **Merge Request Widgets**: `perfgate` generates a `metrics` report that GitLab can display in the MR widget.
2.  **Job Artifacts**: You can download the full Markdown report from the job artifacts.
3.  **Baseline Server**: The execution history will be visible on your `perfgate` dashboard.

## Best Practices

- **Authoritative Runners**: Always run performance checks on "tagged" GitLab runners with dedicated hardware to minimize noise.
- **Noise Policy**: Configure `noise_policy = "warn"` in your `perfgate.toml` for benchmarks that are inherently unstable in CI environments.
