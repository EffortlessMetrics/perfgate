# Benchmark Recipe Selection

`perfgate init --suggest-benches` appends commented benchmark recipes to
`perfgate.toml`. They are reviewable starting points, not policy. Pick one,
edit it to measure this repository, run it locally, and promote a baseline only
after the first result looks representative.

The important question is not which command perfgate can run. It is whether the
workload should be a smoke check, advisory signal, PR gate, paired benchmark,
or structured decision input.

## Recipe Catalog

| Recipe | Best for | Bad for | Expected noise | Recommended mode | PR blocking posture | Paired-mode hint |
|--------|----------|---------|----------------|------------------|---------------------|------------------|
| `rust-cli-smoke` | CLI startup, argument parsing, command wiring | steady-state throughput, parser hot loops, compile-heavy checks | low to medium | advisory until calibrated, then gate if the workload matters | block only after baseline and signal maturity are proven | use paired mode if host or startup variance dominates |
| `rust-workspace-advisory` | broad workspace health, expensive integration paths | first-hour gates, isolated attribution | medium to high | advisory until calibrated; split into smaller gates later | do not block until compile/test noise is understood | use paired mode when CI runner drift changes the verdict |
| `node-command` | dedicated Node benchmark scripts with stable input | package install, network calls, mixed build/test/perf commands | low to medium | advisory until calibrated; gate only stable scripts | block only deterministic benchmark scripts | use paired mode if JIT warmup or runner variance dominates |
| `python-command` | dedicated Python benchmark scripts with fixed input and environment | dependency installation, network calls, correctness-only test runs | medium when interpreter startup or environment setup dominates | advisory until calibrated; gate only stable workloads | block only after repeat count and baseline maturity are proven | use paired mode if interpreter startup or host variance dominates |
| `http-smoke` | local HTTP handlers, smoke latency, simple service endpoints | internet calls, shared staging services, unisolated dependencies | medium to high unless service and host are isolated | advisory first; gate only local isolated endpoints | block only after service startup and network noise are controlled | use paired mode when service startup or runner networking dominates |
| `generic-command` | language-neutral command benchmarks with stable local input | external services, commands that mix setup with runtime | unknown until calibrated | advisory until signal maturity is proven | block only after baseline and signal maturity are proven | use paired mode if repeated local runs disagree |

## Generate A Specific Recipe

Auto-detection is conservative:

```bash
perfgate init --ci github --profile standard --suggest-benches
```

You can request a recipe explicitly:

```bash
perfgate init --ci github --profile standard --suggest-benches rust-cli-smoke
perfgate init --ci github --profile standard --suggest-benches rust-workspace-advisory
perfgate init --ci github --profile standard --suggest-benches node-command
perfgate init --ci github --profile standard --suggest-benches python-command
perfgate init --ci github --profile standard --suggest-benches http-smoke
perfgate init --ci github --profile standard --suggest-benches generic-command
```

Compatibility aliases remain available for the earlier profile names:

```bash
perfgate init --suggest-benches rust-cli
perfgate init --suggest-benches rust-workspace
perfgate init --suggest-benches node
perfgate init --suggest-benches python
```

## Choosing The First Benchmark

Use a first benchmark that is:

- close to the workload the team cares about;
- deterministic enough that repeated local runs are similar;
- fast enough that developers will actually run it;
- isolated from package installation, network calls, and mutable services; and
- narrow enough that a failure suggests a review action.

Keep the first benchmark advisory when you do not yet know its noise. Promote
it to a blocking gate only after baseline and signal maturity are proven.

## Anti-Patterns

Avoid these as first required gates:

| Anti-pattern | Why it is risky | Better first step |
|--------------|-----------------|-------------------|
| Compile-heavy workspace command | compile cache and runner state can dominate the measurement | keep advisory or split to one package/workload |
| Network-heavy command without isolation | internet, staging, and shared service variance can look like code change | use a local isolated service or mark advisory |
| Correctness test suite as performance signal | failures mix correctness, setup, and performance | extract a deterministic benchmark command |
| Tiny runtime | timer granularity and scheduler noise can dominate | increase workload size or repeat count |
| No warmup for startup-sensitive workloads | startup and steady-state effects are mixed accidentally | decide whether startup is the actual target |
| Mutable external input | data changes can look like performance movement | vendor or generate stable input |
| Broad command with no owner | reviewers cannot tell what to fix | split into a workload with a clear owner |

## Promotion Guidance

Do not promote a baseline just because the command ran once. First ask:

```text
Is this the workload we want to protect?
Did repeated local runs look stable?
Would a failure tell a reviewer what to inspect?
Should this be advisory until calibrated?
Would paired mode be safer?
```

After the answer is yes, promote and prove the baseline-required path:

```bash
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

## When To Escalate

Use the smallest surface that answers the review question:

| Situation | Better next step |
|-----------|------------------|
| one stable workload crosses a threshold | local or CI gate |
| repeated runs disagree | keep advisory, increase samples, or use paired mode |
| compile/setup dominates | split the workload or keep advisory |
| throughput improves but memory regresses | structured decision |
| local probe worsens but dominant scenario improves | probe-backed decision |
| the team needs retained decisions | optional server ledger |

Server ledger mode is never required for local correctness. Local receipts,
baselines, reports, action summaries, and decision bundles remain the review
contract.
