# PERFGATE-PROP-0002: Guided adoption

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-13
Target milestone: 0.18.0
Linked specs: PERFGATE-SPEC-0007-guided-adoption-contract
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: plans/0.18.0/guided-adoption.md
Support/status impact: docs/status/PRODUCT_CLAIMS.md should add guided-adoption, probe-backed decision, action reproduction, and server-ledger operations claims
Policy impact: no new policy rows by default; policy ledgers remain source of truth for governed exceptions and public surface

## Problem

perfgate is no longer missing the core product. It has first-run setup, local
checks, structured decisions, decision bundles, GitHub Action mode, optional
server ledger history, debt summaries, Rust 1.95 governance, public-surface
policy, no-panic and file-policy ledgers, release proof, and a source-of-truth
spec stack.

The remaining adoption risk is different: the product is strong for maintainers
who lived through the implementation history, but a cold user can still meet it
as a large tool with many surfaces.

A new user needs a boring path:

```text
install -> doctor -> init -> check -> promote baseline -> CI
```

Then the same user needs a clear way to grow:

```text
action gate -> structured decision -> probes -> tradeoff policy -> server ledger
```

If those paths are not explicit and proven, perfgate risks looking like a
governance-heavy benchmarking project instead of a guided performance-decision
product.

## Users and surfaces

- Cold CLI users need to install, initialize, run a local gate, promote a
  baseline, and understand what to commit without learning the full
  architecture.
- GitHub Action users need failed and warning runs to explain the local
  reproduction command, artifacts, and next step.
- Reviewers need decision artifacts that explain what changed, why policy
  accepted or rejected it, and whether review is required.
- Library users need probe instrumentation that is easy to add without turning
  perfgate into a profiler.
- Team operators need optional server-ledger guidance for storage, keys,
  export, pruning, audit events, and CI upload failure behavior.
- Maintainers and agents need product claims, specs, examples, tests, and
  handoffs to point at the same receipt-based truth.

## Success criteria

- A cold user can follow the first-hour path and know what files were created,
  what artifacts were written, what to commit, and what not to commit.
- The adoption ladder is documented as local gate, GitHub Action gate,
  structured decision, and server ledger, with commands, config, artifacts,
  failure examples, and next steps for each level.
- Structured decision outcomes are teachable through examples for pass, fail,
  warn with accepted tradeoff, review-required, missing evidence, and high
  noise.
- Probe instrumentation is taught as a tradeoff lens, not as profiling, and
  includes minimal JSONL, Rust helper, tracing, and Criterion paths.
- The probe-to-decision path is executable through tests or deterministic
  fixtures that prove ingest, probe compare, decision evaluate, and decision
  bundle behavior.
- GitHub Action summaries and logs explain verdict, failed metric or budget,
  artifacts, local reproduction, baseline bootstrap, decision mode, and
  review-required behavior.
- Server ledger operations remain optional and have a runbook that covers
  storage, keys, export, pruning, audit, dashboard expectations, and CI upload
  failure semantics.
- Product claims map the guided adoption path to support tiers, proof commands,
  linked specs, linked docs, tests, and artifacts.
- The lane closes with a handoff that records what changed, what proof passed,
  which claims changed, and what remains intentionally deferred.

## Proposed shape

This lane turns the existing product surfaces into a guided adoption path.

The product ladder is:

| Level | User question | Product surface |
|-------|---------------|-----------------|
| Local gate | Did this local change regress a benchmark? | `perfgate check`, local baselines |
| GitHub Action gate | Can CI reproduce and explain the same gate? | repository action and artifact upload |
| Structured decision | Did this local regression buy a larger workload improvement? | scenario, tradeoff, decision receipts |
| Server ledger | What performance debt are we accepting over time? | optional decision ledger, debt, export, prune |

The lane should land in small PRs:

- proposal and spec first;
- plan and active goal manifest next;
- docs and examples for first-hour UX, decision outcomes, probes, and server
  operations;
- executable smoke or fixture proof where the docs make a support claim;
- action failure-copy improvements where CI needs to explain itself;
- product-claim updates and narrow freshness checks after the claims exist;
- closeout handoff and archived goal manifest at the end.

## Alternatives considered

### Add more README sections

Rejected. The README should stay an entry point. The guided path needs durable
docs, examples, specs, status claims, tests, and handoffs that can be reviewed
without turning the README into a full manual.

### Treat server mode as the advanced default

Rejected. perfgate's correctness model is receipts-first. The server is useful
team infrastructure, but local receipts and portable bundles remain the
primary contract.

### Make probes the first required workflow

Rejected. Probes are a differentiator, but a user should get value from
`check` before adding instrumentation. Probes should become the next lens when
reviewers need to explain where work moved.

### Hide governance from user docs

Rejected. Governance should not be the product pitch, but it is still the proof
that release claims, policy gates, public surface, and action behavior are
reviewed. User docs should frame governance as checked evidence, not as the
reason to adopt the tool.

### Build a broad semantic graph checker first

Rejected. The useful next checker is narrow: prevent stale planned spec links
when concrete spec files exist. Full claim/spec/policy graph completeness can
wait until the adoption lane has more artifacts.

## Specs to create or update

- `PERFGATE-SPEC-0007-guided-adoption-contract`

`PERFGATE-SPEC-0006-policy-ledger-contracts` remains reserved for the policy
ledger follow-up already identified by the spec-governance closeout.

## Architecture decisions needed

No new ADR is required at lane start. This work relies on existing durable
decisions:

- `PERFGATE-ADR-0002-receipts-first-performance-decisions`
- `PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional`

Add an ADR only if the lane changes the receipts-first model, public crate
surface, or server optionality boundary.

## Product claims affected

This lane should add or update claims for:

- first-hour local adoption path;
- staged adoption levels;
- probe-backed tradeoff explanation;
- GitHub Action local reproduction and failure copy;
- optional team decision-ledger operations.

The claim map remains the support-tier proof surface. It should link to this
proposal, the guided adoption spec, docs, tests, examples, and proof commands
instead of duplicating those artifacts.

## Evidence plan

Documentation, proposal, spec, plan, and status PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Product and test PRs should add the relevant subset of:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Specific adoption proof should cover:

- generated cold project first-hour smoke;
- probe JSONL or helper emission through ingest and decision evaluation;
- decision outcome examples with local reproduction commands;
- action summary/failure-copy checks;
- server-ledger operations docs and CLI proof where practical.

## Risks

- The lane could produce more docs without executable proof.
- Examples could become stale if they are not connected to doc-test,
  schema-compat, action-check, or CLI tests.
- Probe docs could make perfgate look like a profiler instead of a
  receipt-backed decision tool.
- Server ledger docs could imply the server is required for correctness.
- Product claims could drift unless the claim map links concrete specs and
  proof commands.

## Non-goals

- Do not add a new public crate.
- Do not reduce the five public crates in this lane.
- Do not make the server required for local correctness.
- Do not publish crates, create tags, or create GitHub releases.
- Do not make probes required for the basic local gate.
- Do not duplicate policy ledger rows or release-readiness matrices in this
  proposal.
- Do not build a full semantic documentation graph checker before the narrow
  freshness rule exists.

## Exit criteria

This proposal is complete when:

- the guided adoption spec is accepted;
- the guided adoption plan and active goal manifest exist;
- the first-hour path is documented and backed by a smoke fixture or equivalent
  proof;
- decision outcome examples exist for common pass, fail, warn, and
  review-required states;
- probe instrumentation docs explain JSONL, Rust helper, tracing, Criterion,
  ingest, compare, and decision wiring;
- the probe-to-decision path has executable proof;
- action failure output explains local reproduction and decision artifacts;
- server-ledger operations have a runbook;
- product claims cover the supported adoption surfaces;
- the stale planned-spec link checker exists; and
- a closeout handoff archives the goal manifest and records passed proof,
  affected claims, remaining deferred work, and next operators' context.
