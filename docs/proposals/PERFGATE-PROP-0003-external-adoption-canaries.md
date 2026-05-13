# PERFGATE-PROP-0003: External adoption canaries

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-13
Target milestone: 0.18.0
Linked specs: PERFGATE-SPEC-0007-guided-adoption-contract, PERFGATE-SPEC-0004-user-devex-paved-road, PERFGATE-SPEC-0003-performance-decision-contract
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan:
Support/status impact: docs/status/PRODUCT_CLAIMS.md should link successful canary evidence to first-hour, staged-adoption, action reproduction, probe-backed decision, and optional ledger claims when those canaries exist
Policy impact: no new policy rows by default; canaries should not change public-surface, no-panic, generated-file, workflow, or dependency policy unless a canary exposes a concrete repo-policy gap

## Problem

perfgate is now internally coherent. The repo has the first-hour path,
adoption ladder, structured decisions, decision bundles, action failure
examples, optional server ledger runbook, server ledger smoke, wrapper
absorption proof, source-doc checks, product-claim checks, public-surface
policy, and a 0.18 adoption-readiness snapshot.

That proves the product in the repo. It does not yet prove the first contact
experience in real projects maintained by people who did not build perfgate.

The remaining adoption risk is external friction:

```text
install friction
init friction
first failure clarity
baseline promotion clarity
artifact confusion
CI summary clarity
probe instrumentation confusion
ledger operations confidence
```

If those are only tested against synthetic fixtures, perfgate can still be
correct and feel confusing in a real repository.

## Users and surfaces

- Cold CLI users need to install, initialize, run, promote, and rerun without
  reading the architecture docs first.
- Repository maintainers need to understand what generated files to commit and
  which artifacts stay transient.
- GitHub Action users need the first failed CI run to explain whether they are
  missing a baseline, seeing a real regression, or hitting setup friction.
- Reviewers need decision receipts and action summaries that map to the same
  local reproduction commands.
- Advanced users need probes to feel like stable tradeoff lenses, not
  profiler-style instrumentation chores.
- Team operators need confidence that optional ledger mode can be evaluated
  without making the server part of correctness.
- Maintainers and agents need durable canary evidence to decide which docs,
  defaults, examples, or tests deserve follow-up changes.

## Success criteria

- Three to five external canaries are selected before product changes are made
  for this lane.
- Each canary states the project shape it proves and the surfaces under test.
- Each canary records the exact commands run, generated files, artifacts,
  first failure, CI summary, and any confusion found.
- At least one canary proves a small Rust CLI.
- At least one canary proves a larger Rust workspace.
- At least one canary proves a non-Rust command benchmark.
- At least one canary exercises noisy or unstable benchmark behavior.
- At least one canary exercises GitHub Action-only adoption.
- Follow-up fixes are filed or landed only when canary evidence shows a
  concrete adoption failure.
- Product claims are updated only after canary evidence exists; this proposal
  does not make new support claims by itself.

## Proposed shape

Use canaries as external adoption receipts. A canary is not a benchmark
leaderboard and not a synthetic fixture. It is a small evidence packet for a
real repository shape.

The initial canary set should cover:

| Canary | Repository shape | Primary question |
| --- | --- | --- |
| Small Rust CLI | Single binary or small workspace | Can a maintainer get value from install, init, check, promote, and CI without extra architecture context? |
| Larger Rust workspace | Multi-crate repo with existing test or benchmark commands | Does perfgate remain clear when commands, baselines, and artifacts multiply? |
| Non-Rust command benchmark | Script, Node, Python, or shell command | Does the command-benchmark path work without Rust-specific assumptions? |
| Noisy benchmark | Repo with intentionally unstable timing or resource use | Does the output guide rerun, paired mode, threshold tuning, or warn/fail policy instead of creating false confidence? |
| Action-only adoption | Repo where CI is the first meaningful integration | Does the action summary explain missing baselines, artifacts, local reproduction, and next steps? |

Each canary should produce a short evidence note with:

- repository and commit under test;
- host and runner context;
- install command and result;
- `perfgate init` command and generated file list;
- first `perfgate check` result and artifact list;
- baseline promotion decision and committed-file guidance;
- CI summary excerpt or local equivalent;
- whether docs were enough;
- what confused the operator;
- follow-up issue, PR, or explicit no-change decision.

Canary evidence can live under `docs/audits/` or `docs/handoffs/` depending on
whether it is a one-off observation or a lane closeout. The canary documents
should link to this proposal and to the guided-adoption spec instead of
duplicating product contracts.

## Alternatives considered

### Add more synthetic tests first

Rejected. The repo already has strong synthetic and fixture proof for first
hour, structured decisions, action summaries, and server ledger operations. The
gap now is whether those surfaces feel obvious in real projects.

### Treat one dogfood repo as enough

Rejected. One repository can prove that perfgate works somewhere. It cannot
show whether the path is obvious across small CLIs, larger workspaces,
non-Rust commands, noisy benchmarks, and action-first adoption.

### Convert canaries into permanent CI dependencies

Rejected for the first lane. External repositories can be unavailable,
expensive, or noisy. The first canaries should produce evidence and targeted
fixes. Permanent cross-repo CI can be considered only after the canary shape is
stable and low-friction.

### Add new performance primitives before canaries

Rejected. The current product is capable enough to test the adoption
experience. New primitives would obscure whether existing install, init,
check, decision, probe, action, and ledger surfaces are already understandable.

## Specs to create or update

No new spec is required at lane start. This lane exercises existing behavior
contracts:

- `PERFGATE-SPEC-0007-guided-adoption-contract`
- `PERFGATE-SPEC-0004-user-devex-paved-road`
- `PERFGATE-SPEC-0003-performance-decision-contract`
- `PERFGATE-SPEC-0005-release-proof-contract`

Create a new spec only if canary evidence shows a behavior contract gap that
cannot be expressed as docs, tests, examples, status claims, or action output.

## Architecture decisions needed

No new ADR is required at lane start. This work relies on existing decisions:

- receipts-first performance decisions;
- local receipts first, optional server ledger;
- public crates as contracts and modules as architecture boundaries.

Add an ADR only if canary evidence would change one of those durable
architecture boundaries.

## Product claims affected

Canary evidence may later strengthen or qualify claims for:

- first-hour local adoption path;
- staged adoption levels;
- GitHub Action local reproduction and failure summary;
- probe-backed tradeoff explanation;
- optional team decision-ledger operations;
- platform and metric support boundaries.

Until evidence exists, `docs/status/PRODUCT_CLAIMS.md` should continue to map
claims to in-repo proof and should not imply broad external adoption coverage.

## Evidence plan

Canary planning PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Canary execution PRs should include the relevant subset of:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Each canary must include its own evidence note with:

```text
repo
commit
host/runner
commands
generated files
artifacts
first failure or first pass
operator confusion
follow-up decision
```

## Risks

- Canaries can become anecdotal if they do not record exact commands and
  artifacts.
- External repositories can drift, making old evidence stale.
- Operators may overfit docs or defaults to one repo shape.
- Noisy benchmark canaries can create false confidence if the evidence does
  not distinguish signal from runner variance.
- Action-only canaries can hide local usability gaps if the local reproduction
  command is not actually run.

## Non-goals

- Do not publish crates, move release tags, or update action aliases.
- Do not make the server required for correctness.
- Do not change the five public crates.
- Do not add a new performance primitive by default.
- Do not turn probes into profiling.
- Do not commit external repository contents into this repo.
- Do not make external canaries required CI before the canary process is proven
  stable.
- Do not duplicate policy ledgers or release matrices in canary notes.

## Exit criteria

This proposal is complete when:

- a PR-sized canary implementation plan exists;
- three to five canary targets or target shapes are selected;
- at least three canary evidence notes are recorded;
- at least one canary runs through first-hour local adoption;
- at least one canary runs through GitHub Action adoption or an equivalent
  action-summary rehearsal;
- at least one canary records noisy benchmark guidance or threshold tuning;
- follow-up fixes from canary evidence are landed, deferred with rationale, or
  filed as tracked work;
- product claims link canary evidence where it strengthens support claims; and
- a handoff records what external adoption proved, what remains unproven, and
  what should not be inferred from the canaries.
