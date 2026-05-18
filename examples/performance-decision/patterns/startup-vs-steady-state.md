# Startup Slower But Steady-State Faster

## Shape

Initialization gets slower because work moves earlier, while steady-state
operations get faster.

```text
startup_ms          regressed 6.0%
steady_state_ops    improved 15.0%
```

## Why It Matters

This tradeoff depends on how the product is used. A command-line tool that
starts often may care about startup more. A long-lived service may accept a
startup cost for better steady-state throughput.

## Receipts To Inspect

```text
compare.json
scenario.json
decision.md
decision.index.json
```

## Reviewer Action

Use scenarios to weight startup and steady-state separately. If startup is part
of the user promise, keep it as a gate or require review. If steady-state is the
dominant workload, document why the startup cost is acceptable.

```bash
perfgate decision evaluate --config perfgate.toml
```

## Do Not

- Do not collapse startup and steady-state into one benchmark if reviewers need
  to reason about both.
- Do not make a compile-heavy setup command the startup benchmark.
- Do not accept the tradeoff without naming the workload that matters.
