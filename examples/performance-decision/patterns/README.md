# Decision Pattern Pack

This pack gives reviewers recognizable structured-decision shapes before they
need to author a full scenario or tradeoff policy from scratch. It complements
the outcome gallery: outcomes describe final verdicts, while patterns describe
the review conversation that usually leads to a decision.

Use these examples when `perfgate decision suggest` says a structured decision
may help, or when a PR has more than one meaningful metric movement.

## Patterns

| Pattern | Use when | First reviewer question |
|---------|----------|-------------------------|
| [Latency regression with throughput improvement](latency-vs-throughput.md) | request latency worsens but batch throughput improves | Does the workload that improved matter more for this change? |
| [Memory regression with runtime improvement](memory-vs-runtime.md) | runtime improves by spending more memory | Is the memory increase inside policy for the deployment target? |
| [Startup slower but steady-state faster](startup-vs-steady-state.md) | initialization cost moves into a better steady-state path | Which phase matters for users and release policy? |
| [Probe regression with dominant workload improvement](probe-backed-tradeoff.md) | local work moved but the weighted scenario improved | Did the dominant workload improve enough to accept the local cost? |
| [Noise too high for a decision](noisy-no-decision.md) | evidence moves but noise makes the judgment unsafe | Should this be rerun paired or kept advisory? |

## Review Shape

Each pattern follows the same contract:

- what moved;
- why it matters;
- which receipts to inspect;
- what the reviewer should run next; and
- what not to do.

Local receipts remain the correctness contract. The server ledger is optional
team history and is not required for any example in this pack.
