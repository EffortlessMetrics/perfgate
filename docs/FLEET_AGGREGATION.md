# Fleet Aggregation

`perfgate aggregate` combines run receipts from matrix or fleet runners into a
single `perfgate.aggregate.v1` receipt and an aggregate run receipt. Use it when
one benchmark is intentionally measured on more than one runner and you need one
policy decision for CI.

## Policies

Use the simplest policy that matches the release risk:

| Policy | Use when |
|--------|----------|
| `all` | every runner is authoritative and any failure should block |
| `majority` | runners are equivalent and one isolated failure should not block |
| `quorum` | you need a required pass ratio without per-runner weights |
| `weighted` | runner classes have different trust levels |
| `fail_if_n_of_m` | CI is sharded and a fixed number of failures should block |

## Inverse-Variance Weighting

`--weight-mode inverse_variance` is for matrix CI where runner variance is not
equal. It downweights runners with high wall-time variance and records each
runner's `sample_count`, `wall_ms_variance`, `effective_weight`, and
`outlier_reason` in the aggregate receipt.

Use it for:

- self-hosted stable runners mixed with shared hosted runners
- matrix CI where one OS or runner image is known to be noisy
- observe-first fleet gates where a noisy failure should not dominate stable
  runners

Do not use it as a substitute for enough samples. The aggregate receipt warns
when a runner has fewer than five measured samples, or when there are not enough
samples to estimate variance and the variance floor is used.

```bash
perfgate aggregate \
  artifacts/linux/perfgate.run.v1.json \
  artifacts/windows/perfgate.run.v1.json \
  artifacts/macos/perfgate.run.v1.json \
  --policy weighted \
  --weight-mode inverse_variance \
  --quorum 0.75 \
  --variance-floor 1.0 \
  --runner-class github-hosted \
  --lane pr \
  --out artifacts/perfgate/aggregate.json
```

## Runner Classes

Treat runner class as operator metadata, not a magic trust override:

| Runner class | Suggested use |
|--------------|---------------|
| `self-hosted-stable` | authoritative gate if hardware and load are controlled |
| `github-hosted` | useful gate with enough repeats and variance-aware aggregation |
| `dev-laptop` | local diagnosis or observe-only trends |

For stable self-hosted runners, start with `all` or `weighted` with configured
weights. For GitHub-hosted shared runners, prefer `weighted` plus
`inverse_variance` and at least five measured samples per runner. For laptops,
prefer local `paired` runs and avoid making them required gates.

## Matrix CI Example

Each matrix job should write a run receipt as an artifact. A final aggregation
job downloads those receipts and makes the policy decision:

```bash
perfgate aggregate "artifacts/**/perfgate.run.v1.json" \
  --policy weighted \
  --weight linux-x86_64=0.6 \
  --weight windows-x86_64=0.2 \
  --weight macos-aarch64=0.2 \
  --quorum 0.70 \
  --out artifacts/perfgate/aggregate.json
```

When the aggregate verdict fails, the command exits `2`; use normal CI shell
control flow if later artifact upload or reporting steps must still run.
