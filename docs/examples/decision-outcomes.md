# Decision Outcome Gallery

This gallery helps reviewers recognize the common shapes of a perfgate
decision. It is not a second spec. The behavior contract lives in
[`PERFGATE-SPEC-0003`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
and [`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md).

Use these examples when a CI summary, `decision.md`, or decision bundle looks
unfamiliar.

For examples of common tradeoff review conversations before the final verdict,
see the
[`examples/performance-decision/patterns`](../../examples/performance-decision/patterns/README.md)
pack.

## Common Review Packet

Structured decisions are review packets. A complete local packet usually has:

```text
artifacts/perfgate/
  compare.json
  probe-compare.json
  scenario.json
  tradeoff.json
  decision.md
  decision.index.json
  decision-bundle.json
```

The exact paths may be per-benchmark subdirectories when `check --all` runs.
`decision.index.json` is the machine-readable manifest that tells humans,
actions, servers, and agents where the supporting receipts live.

## Local Reproduction

For a standard decision-enabled run:

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

For the deterministic fixture in this repo:

```bash
perfgate ingest probes --file examples/performance-decision/probes-baseline.jsonl --out artifacts/perfgate/large-file/probes-baseline.json
perfgate ingest probes --file examples/performance-decision/probes-current.jsonl --out artifacts/perfgate/large-file/probes-current.json
perfgate decision evaluate --config examples/performance-decision/perfgate.toml
```

## Outcome Index

| Outcome | Shape | Reviewer action |
|---------|-------|-----------------|
| [Pass](../../examples/performance-decision/outcomes/pass.md) | Workload stays inside policy. | Merge from a performance perspective if other review is clean. |
| [Fail](../../examples/performance-decision/outcomes/fail.md) | A required metric regresses and no accepted tradeoff applies. | Fix, explain, or intentionally update policy/baseline in a separate review. |
| [Warn with accepted tradeoff](../../examples/performance-decision/outcomes/warn-accepted-tradeoff.md) | A local regression is accepted because configured compensating evidence passes. | Review the policy rule and evidence, then decide whether the tradeoff is intentional. |
| [Review required](../../examples/performance-decision/outcomes/review-required.md) | Evidence could support a tradeoff, but policy says a human must inspect it. | Inspect missing/noisy evidence before treating it as accepted. |
| [Missing evidence](../../examples/performance-decision/outcomes/missing-evidence.md) | A receipt, probe, or metric required by policy is absent. | Reproduce with the missing artifact path fixed or change policy in a separate PR. |
| [High noise](../../examples/performance-decision/outcomes/high-noise.md) | Evidence exists but is too noisy for automatic acceptance. | Rerun under steadier conditions or require human approval. |

## What A Reviewer Should Check

Before accepting a structured decision:

- read `decision.md` first;
- follow `decision.index.json` to the receipts when the summary is surprising;
- check whether the final verdict is `pass`, `warn`, `fail`, or
  review-required;
- verify that accepted tradeoffs name a configured rule;
- inspect probe deltas when the tradeoff depends on a named probe;
- confirm the local reproduction command is present; and
- bundle the decision when it needs to travel with a PR, release, issue, audit,
  or agent handoff.
