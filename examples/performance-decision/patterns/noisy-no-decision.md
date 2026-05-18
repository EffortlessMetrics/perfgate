# Noise Too High For A Decision

## Shape

Metrics move, but the signal is noisy enough that the decision would create
false confidence.

```text
wall_ms    changed 4.0%
cv         18.0%
status     noisy
```

## Why It Matters

Noise is not an accepted tradeoff. If the evidence is unstable, reviewers need a
better run, paired mode, or advisory treatment before they can accept or reject
the change.

## Receipts To Inspect

```text
run.json
compare.json
report.json
repair_context.json
decision.md
```

## Reviewer Action

Keep the benchmark advisory, increase samples, or run paired mode before making
the result block a PR.

```bash
perfgate doctor signal --config perfgate.toml
perfgate calibrate --config perfgate.toml --bench parser --emit-patch
perfgate paired --name parser --baseline-cmd "cargo run -- benchmark-old" --current-cmd "cargo run -- benchmark-new" --repeat 10 --out artifacts/perfgate/parser/compare.json
```

## Do Not

- Do not treat noise as a regression or an accepted tradeoff.
- Do not promote a new baseline just to silence noisy evidence.
- Do not tighten thresholds until receipts show stable signal.
