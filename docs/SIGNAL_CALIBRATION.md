# Signal Calibration

perfgate is useful only when a failing gate means "investigate performance" and
not "the runner was noisy." Start with conservative policy, collect receipts,
then tighten only after the benchmark proves stable.

This guide explains how to pick first thresholds, repeat counts, runner classes,
noise policy, paired mode, and escalation paths.

## Start Conservative

For a first local gate, prefer the generated standard profile:

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
warn_factor = 0.50
noise_threshold = 0.10
noise_policy = "warn"
out_dir = "artifacts/perfgate"
baseline_dir = "baselines"
```

This means:

- fail by budget when a lower-is-better metric regresses by more than 20%;
- warn by budget when it regresses by at least 10%;
- mark high-noise metrics as warnings when current-run coefficient of
  variation is above 10%;
- keep noisy evidence visible instead of silently accepting it.

Do not make the first threshold tight just because the first run looked stable.
Promote a baseline only after the command represents the workload you care
about.

```bash
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

## Thresholds Are Policy

A threshold is the amount of regression the project is willing to tolerate. It
is not a statistical confidence value and should not be auto-loosened by a
baseline refresh.

Use this rule of thumb:

| Benchmark shape | First fail threshold | First warn threshold |
|-----------------|----------------------|----------------------|
| stable CPU-bound command | 10-20% | half the fail threshold |
| normal CLI or integration command | 20-30% | half the fail threshold |
| noisy shared-runner command | 30% or advisory only | half the fail threshold |
| external service or network path | do not gate by default | advisory only |

Tighten a threshold only when recent receipts show low variance and the tighter
budget would not have created false positives.

## Noise Policy

`noise_threshold` is the coefficient-of-variation cap for trusting the current
run. A value of `0.10` means 10%.

Use the policies this way:

| Policy | Use when | Effect |
|--------|----------|--------|
| `warn` | the benchmark is useful but sometimes noisy | noisy metrics become warnings |
| `skip` | noise should remove a metric from the verdict | noisy metrics become skipped |
| `ignore` | another system owns noise handling | noise does not change status |

The generated config uses `warn` because it is the safest first-hour default:
users see that the signal is noisy instead of treating it as a clean pass.

## Host Class

Do not mix baselines across materially different hosts. Prefer one host class
for required gates and use other hosts for advisory checks.

| Host class | Good use | Calibration guidance |
|------------|----------|----------------------|
| developer laptop | local setup and debugging | do not treat as canonical unless the team agrees |
| GitHub-hosted runner | pull-request advisory or broad CI coverage | keep thresholds conservative and noise visible |
| pinned self-hosted runner | required performance gate | collect enough history to tighten thresholds |
| release calibration runner | release proof and baseline refresh | use high repeat counts and explicit review |

If CI and local results disagree, first check host mismatch and artifact paths
before changing thresholds.

## Repeat And Warmup

Start with enough repetitions to expose obvious noise, not so many that a first
run becomes painful.

| Situation | Suggested config |
|-----------|------------------|
| first local gate | `repeat = 7`, `warmup = 1` |
| stable required CI gate | `repeat = 10..15`, `warmup = 1..3` |
| release calibration | at least 15 samples |
| high setup cost benchmark | increase warmup before increasing repeat |
| very noisy paired comparison | use `perfgate paired` with significance checks |

If a benchmark is still noisy after more samples, treat that as information.
Split mixed workloads, remove external dependencies, or keep the benchmark
advisory.

## When To Use Paired Mode

Use paired mode when the question is "did implementation B beat implementation
A under the same runner conditions?" It interleaves baseline and current
commands so short-term runner drift affects both sides.

```bash
perfgate paired \
  --name parser \
  --baseline-cmd "./bench-old" \
  --current-cmd "./bench-new" \
  --repeat 10 \
  --significance-alpha 0.05 \
  --require-significance \
  --max-retries 3 \
  --cv-threshold 0.50 \
  --out artifacts/perfgate/compare.json
```

Use normal `check` for the day-to-day checked-in baseline gate. Use paired mode
for noisy A/B investigations, release evidence, or reviewer questions that need
higher confidence.

## When To Use Structured Decisions

Use structured decisions when a local regression may be acceptable because a
more important workload improved.

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

Good examples:

- parser tokenization regresses 2%, but batch parse improves 10%;
- memory rises inside a configured cap, but wall time improves for the dominant
  scenario;
- CI sees a warning and reviewers need the receipt trail before accepting it.

Do not use structured decisions to hide unstable measurements. If evidence is
too noisy, configure decision policy to require review instead of automatic
acceptance.

## When Not To Gate

Keep a benchmark advisory until it has a stable enough signal.

Do not make it a required gate when:

- the command depends on a remote service or shared cache;
- the benchmark mixes unrelated workloads;
- the runner changes CPU, memory, or power behavior between runs;
- repeated receipts show high CV and no clear fix;
- the workload is not the one users or reviewers care about;
- the team cannot explain what action a failure should trigger.

In those cases, keep receipts, use `noise_policy = "warn"`, and promote the
benchmark only after the signal is actionable.

## Baseline Refresh

Baseline refresh moves the comparison point. It should not relax policy.

Before promoting a new baseline, confirm:

- the benchmark command still represents the intended workload;
- artifacts come from the agreed host class;
- the run is not marked high-noise;
- the threshold did not change in the same commit unless the PR explains why;
- reviewers can reproduce the check locally or from the CI command.

For repository-level baseline governance, see
[`BASELINE_POLICY.md`](BASELINE_POLICY.md). For long-running noise history, see
[`FLAKINESS.md`](FLAKINESS.md). For interleaved A/B checks, see
[`PAIRED_BENCHMARKING.md`](PAIRED_BENCHMARKING.md).
