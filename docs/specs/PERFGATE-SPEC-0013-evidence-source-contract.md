# PERFGATE-SPEC-0013: Evidence source contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-19
Milestone: 0.21.0
Behavior version: evidence-source-contract.v1
Product surface: evidence adapters, imported benchmark receipts, maturity, policy posture, review packets, GitHub Action summaries, adoption packs, external canaries
CI surface: docs-source-check, product-claims-check, doc-test, focused CLI adapter/import tests, policy/report/check tests, action-check, schema-compat if receipt shape changes
Schema impact: no receipt schema change by default; adapters should map into existing receipt shapes or isolated adapter metadata unless a follow-up accepted spec proves a versioned schema change is required
Action impact: no action input, alias, or workflow behavior change by default; actions may run import commands and surface imported evidence through existing summaries
Server impact: server ledger remains optional team history; imported evidence must not require server mode for local correctness
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan: plans/0.21.0/evidence-intake-adoption-packs.md
Linked policy: policy ledgers remain source of truth for governed exceptions, public surface, workflow policy, generated files, and release proof
Support/status impact: product claims should add or promote evidence-intake claims only after adapters, fixtures, docs, Action proof, and canaries land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate is a receipts-first performance decision system. Teams should be able
to keep using benchmark tools they already trust while adding perfgate's
evidence, maturity, policy, review, and Action surfaces on top.

The adoption gap is evidence intake. Real repositories already produce
measurements from tools such as:

```text
hyperfine
Criterion
pytest-benchmark
k6
shell commands
custom JSON
custom CSV
```

Without a source contract, adapters can become unsafe in two opposite ways:

```text
too strict: users must rewrite benchmarks before perfgate can help
too loose: imported numbers look like mature policy evidence without review
```

This spec defines the behavior contract for imported evidence. It keeps
measurement tools outside perfgate and makes adapters responsible for
transparent mapping into perfgate evidence.

## Behavior

Evidence intake MUST preserve the product boundary:

```text
external tools measure
perfgate imports and normalizes evidence
receipts preserve review context
maturity classifies trust
policy decides advisory versus promotion
review packets and Actions explain the next step
```

Adapters MUST be transparent. For every imported result, perfgate SHOULD expose
what source was read, what metric was mapped, what unit and direction were used,
what sample model was available, what host context was preserved, what was
inferred, what could not be inferred, and what review is needed before gating.

Imported evidence MUST remain advisory until normal maturity and policy
surfaces support stronger posture. An adapter MUST NOT silently promote a
baseline, make a benchmark blocking, loosen thresholds, accept a tradeoff, or
require server ledger mode.

## Evidence source model

Each adapter SHOULD model imported evidence with the following fields:

```text
source_kind
source_path
source_version
source_command
benchmark_name
metric_name
unit
direction
sample_model
sample_count
host_context
noise_support
baseline_compatibility
adapter_metadata
inferred_fields
explicit_fields
non_inferences
```

The source model MAY be internal or rendered as isolated adapter metadata. It
SHOULD map into existing perfgate receipts wherever possible. Broad receipt
schema changes require a follow-up accepted spec and schema-compat proof.

## Source kinds

The canonical initial source kinds are:

| Source kind | Meaning |
|-------------|---------|
| `generic_command_json` | User-provided JSON shaped for perfgate import. |
| `hyperfine_json` | JSON output produced by hyperfine command benchmarks. |
| `criterion` | Criterion benchmark output from Rust projects. |
| `pytest_benchmark_json` | JSON output from pytest-benchmark. |
| `k6_summary_json` | k6 summary JSON for HTTP or scripted load/smoke runs. |
| `custom_json` | User-mapped JSON that is not one of the known tool formats. |
| `custom_csv` | User-mapped CSV that is not one of the known tool formats. |

User-facing output MAY use friendly labels, but adapter metadata and tests
SHOULD preserve these source-kind meanings.

## Metric mapping

Adapters MUST map external measurements to explicit metric names, units, and
directions.

Metric direction MUST be explicit or safely inferred from known source
semantics. Examples:

| Metric | Direction |
|--------|-----------|
| wall time, duration, latency, memory, page faults | lower is better |
| throughput, requests per second, operations per second | higher is better |

Ambiguous units or directions MUST produce actionable guidance instead of a
silent guess. The user SHOULD be told which mapping is missing and how to
provide it.

Adapters MUST preserve enough metadata for reviewers to understand whether a
positive or negative delta is good. Imported metrics MUST use the same
direction-aware movement semantics as native perfgate evidence.

## Unit normalization

Adapters SHOULD normalize units into existing perfgate metrics when semantics
match. Examples:

```text
seconds -> wall_ms when the value is elapsed wall time
bytes -> max_rss_kb only when the source is memory usage, not payload size
requests/second -> throughput_per_s when the source is throughput
operations/second -> ops_per_s when the source is operation throughput
```

When a metric does not match an existing perfgate metric, adapters MAY preserve
it as a custom metric with explicit unit and direction metadata. Custom metrics
MUST NOT be treated as first-class supported metrics without product-claim proof.

## Sample model

Adapters SHOULD preserve raw samples when the source provides them. Raw samples
support noise, maturity, calibration, and paired-mode guidance.

When the source provides only summary statistics, the adapter MUST mark limited
noise support. Summary-only imports MAY still be useful evidence, but they MUST
NOT be described as having the same maturity proof as raw-sample receipts.

The adapter SHOULD report:

```text
raw samples available
summary statistics available
sample count
aggregation method
whether variance/CV/noise can be computed
what was lost in conversion
```

## Host context

Adapters SHOULD preserve host context when available, including operating
system, architecture, runner, CPU, memory, runtime, interpreter, container, or
tool version fields provided by the source.

When host context is unavailable, the adapter MUST mark it as unknown. Missing
host context MUST NOT be interpreted as host compatibility.

Imported evidence SHOULD reach host mismatch and maturity surfaces only when
the adapter can provide enough host context or can clearly mark the limitation.

## Baseline compatibility

Imported evidence MAY become current evidence for comparison. It MAY become a
baseline only through the existing explicit promotion path.

Adapters MUST NOT auto-promote baselines. They MUST NOT convert first import
success into mature baseline status. Baseline doctor and policy doctor MUST be
allowed to classify imported baselines as missing, new, immature, stale,
host-mismatched, or high-noise when evidence supports that result.

## Adapter metadata

Adapter metadata SHOULD answer:

```text
which tool produced this evidence
which file was read
which adapter version imported it
which source fields were consumed
which fields were ignored
which fields were inferred
which fields were user-configured
which fields require review
```

Adapter metadata SHOULD be small and stable enough for review. It SHOULD NOT
turn imported evidence into a second source of correctness beside perfgate
receipts. Local receipts remain the correctness contract.

## Non-inferences

Every adapter SHOULD have explicit non-inference text. At minimum:

- a successful source tool run does not prove the benchmark is mature;
- imported evidence does not become blocking policy by default;
- imported evidence does not prove host compatibility when host data is absent;
- imported evidence does not prove baseline quality until baseline maturity is
  evaluated;
- external tool statistics are not automatically equivalent to perfgate
  maturity policy;
- HTTP or k6 smoke output does not prove production capacity;
- pytest correctness success is not performance evidence by itself;
- hyperfine command timing may include shell, setup, cache, or compile overhead;
  and
- server ledger upload remains optional team history unless configured
  otherwise by explicit team policy.

## Commands and surfaces

Implementation SHOULD provide an import surface such as:

```bash
perfgate import --source hyperfine --input hyperfine.json --config perfgate.toml
```

The exact CLI shape belongs in the 0.21 implementation plan and adapter PRs.
Whatever command shape is selected, imported evidence SHOULD be able to reach:

```text
baseline doctor
doctor signal
policy doctor
policy emit-patch
policy review-packet
GitHub Action summary
repair context
product claims
canary freshness
```

If an adapter cannot feed one of these surfaces, it MUST document the gap and
avoid overclaiming support.

## Error behavior

Adapter errors MUST be actionable. They SHOULD classify at least:

| Error class | Required guidance |
|-------------|-------------------|
| missing input file | show the expected path and reproduction command |
| invalid JSON or CSV | show parse context without dumping large payloads |
| unsupported source version | show supported versions or fallback path |
| missing benchmark name | ask for explicit name mapping |
| missing metric | show required metric mapping fields |
| ambiguous unit | ask for explicit unit mapping |
| ambiguous direction | ask for explicit metric direction |
| no samples or summary | explain that no usable measurement was found |
| missing host context | mark host as unknown and explain maturity limits |
| unsupported metric shape | suggest custom JSON/CSV mapping or adapter follow-up |

Errors MUST NOT recommend loosening thresholds, blindly promoting baselines, or
making server ledger mode part of correctness.

## Adapter requirements

### Generic command JSON

Generic command JSON is the lowest-risk intake path because users control the
shape. It SHOULD require explicit metric, unit, and direction mapping unless
the JSON schema names a known perfgate metric unambiguously.

Generic imports SHOULD have:

- positive fixture with raw samples;
- positive fixture with summary-only evidence;
- missing metric error;
- ambiguous unit error;
- ambiguous direction error;
- host-known and host-unknown cases; and
- proof that the imported evidence reaches maturity or review surfaces.

### hyperfine JSON

hyperfine imports SHOULD preserve command name, mean, median, standard
deviation, min, max, user time, system time, and raw runs where available.

The adapter MUST explain that command timing can include setup, shell, cache,
compile, or environment overhead. It SHOULD classify compile-heavy or
setup-heavy commands as advisory unless maturity evidence supports promotion.

### Criterion

Criterion imports SHOULD map only fields whose semantics are clear. Criterion's
statistics SHOULD NOT be collapsed into perfgate maturity claims without
explaining what was preserved and what was lost.

Criterion imports SHOULD preserve benchmark identity, measured unit, sample
information, estimate fields when used, and enough metadata to keep Rust
benchmark review understandable.

### pytest-benchmark JSON

pytest-benchmark imports SHOULD separate correctness test execution from
performance evidence. Passing tests do not prove performance maturity.

The adapter SHOULD preserve Python/runtime context where available and mark
interpreter, environment, fixture, and host limits clearly.

### k6 summary JSON

k6 summary imports SHOULD treat HTTP and load-test output as environment-bound
evidence. They MUST NOT infer production capacity from local, shared-runner, or
uncontrolled network runs.

The adapter SHOULD preserve request rate, latency summaries, error rate, and
scenario labels where available. It SHOULD explain whether output is smoke,
advisory, or candidate policy evidence.

### Custom JSON and CSV

Custom mapping adapters SHOULD require explicit field mapping for metric name,
value, unit, direction, sample identity, and host context when available.

Custom adapters SHOULD fail closed when a mapping would silently invert metric
direction or drop all sample context.

## Adoption packs

Adoption packs SHOULD be reviewable templates, not automatic benchmark
selection. Initial packs SHOULD include:

```text
rust-cli
rust-workspace
python-service
node-tool-action
http-local-smoke
generic-command
```

Each pack SHOULD specify:

```text
benchmark source
expected artifact path
adapter command
suggested starting posture
known bad fits
Action snippet
local reproduction command
baseline path
promotion path
what not to infer
```

Packs MUST NOT silently promote baselines, make gates blocking, loosen
thresholds, require server ledger mode, or claim universal best-practice status.

## External canaries

The lane SHOULD include at least two external canaries:

- one Rust repo with an existing benchmark source such as Criterion, hyperfine,
  or command output; and
- one non-Rust command, JSON/CSV, pytest-benchmark, k6, or HTTP repo.

Each canary SHOULD record:

```text
repo shape
existing benchmark source
adapter command
imported evidence artifact
baseline path
check or compare command
baseline doctor output
signal doctor output
policy doctor output
review packet output
Action summary posture when hosted CI is part of the canary
what confused the user
what was fixed
what it proves
what it does not prove
freshness state
```

Canaries MUST distinguish source-built proof from public release proof.

## Non-goals

- Do not add another benchmark engine.
- Do not add a benchmark scheduler.
- Do not add a dashboard.
- Do not expand public crates by default.
- Do not require server ledger mode.
- Do not auto-promote baselines.
- Do not auto-loosen thresholds.
- Do not make imported evidence blocking by default.
- Do not broadly change receipt schemas without an accepted spec.
- Do not make external benchmark tools part of perfgate correctness.
- Do not treat adapter support as proof that every repo shape is supported.
- Do not reopen the 0.18 release lane, 0.19 evidence-maturity lane, or 0.20
  policy-ergonomics lane unless a real regression appears.

## Required evidence

Documentation-only changes to this spec SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Adapter changes SHOULD add focused proof:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

Action or adoption-pack changes SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
```

Cross-cutting implementation SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

## Acceptance examples

| Example | Result |
|---------|--------|
| A hyperfine import maps elapsed duration to a lower-is-better wall-time metric and records raw runs. | Pass |
| A custom JSON import without a clear unit fails with guidance to provide explicit unit mapping. | Pass |
| A throughput metric imports as higher-is-better and uses direction-aware movement semantics. | Pass |
| A summary-only source marks noise support as limited and remains advisory until maturity evidence supports stronger posture. | Pass |
| A k6 summary import reports HTTP smoke limits and does not claim production capacity. | Pass |
| A pytest-benchmark import separates test success from performance evidence. | Pass |
| Imported evidence reaches policy doctor and review-packet output without making the benchmark blocking. | Pass |
| An adoption pack shows an adapter command, local reproduction command, starting posture, promotion path, and what not to infer. | Pass |
| A missing-host import is treated as host-compatible. | Fail |
| An ambiguous direction import silently assumes lower-is-better. | Fail |
| A first successful import promotes a baseline automatically. | Fail |
| A hyperfine compile-heavy command is marked as safe required-gate evidence without maturity proof. | Fail |
| A k6 local run is described as production capacity evidence. | Fail |
| A server upload failure invalidates local imported receipts by default. | Fail |

## Test mapping

Current or planned proof maps to:

- CLI adapter tests for generic command JSON, hyperfine JSON, Criterion,
  pytest-benchmark JSON, k6 summary JSON, custom JSON, and custom CSV;
- fixture tests for positive imports, bad input, unit mapping, direction
  mapping, sample/noise support, host-known, and host-unknown cases;
- CLI check/policy/report tests for imported evidence reaching maturity,
  policy, review-packet, and repair surfaces;
- action-check fixtures when imported evidence appears in GitHub Action
  summaries;
- docs-source-check for proposal/spec/plan/status links;
- product-claims-check for support tier and proof freshness claims;
- schema-compat if adapter metadata or receipts change; and
- public-surface checks if implementation touches package boundaries.

## Implementation mapping

The evidence source contract is owned by:

- `docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md` for
  lane rationale;
- this spec for source behavior, adapter semantics, proof, and non-inferences;
- the future 0.21 implementation plan for PR sequencing;
- `crates/perfgate-cli` for import commands, adapter mapping, Action-friendly
  output, and local reproduction guidance;
- `perfgate::domain` for shared direction-aware movement semantics when
  imported metrics become compare/report evidence;
- baseline, signal, policy, review-packet, repair-context, and Action summary
  surfaces for imported evidence consumption;
- `docs/status/PRODUCT_CLAIMS.md` and `docs/status/CANARY_MATRIX.md` for
  support mapping and freshness; and
- existing policy ledgers for governed exceptions, public surfaces, workflow
  policy, generated files, and release proof.

This spec may link policy ledgers but MUST NOT copy their rows.

## CI proof

Evidence-intake changes MUST select proof commands by affected surface:

| Surface | Proof |
|---------|-------|
| Proposal/spec/plan/status docs | `docs-check`, `doc-test`, `docs-source-check`, `product-claims-check`, `git diff --check` |
| Adapter parsing and mapping | focused CLI adapter/import tests |
| Metric direction and units | focused adapter fixtures plus direction-aware movement tests |
| Baseline/signal maturity integration | focused baseline and doctor tests |
| Policy and review-packet integration | focused policy/report/check tests |
| GitHub Action summary | `cargo +1.95.0 run -p xtask -- action-check` |
| Receipt/schema impact | `cargo +1.95.0 run -p xtask -- schema-compat` |
| Product claim support | `cargo +1.95.0 run -p xtask -- product-claims-check` |
| Public surface risk | `cargo +1.95.0 run -p xtask -- public-surface --strict` |
| Architecture risk | `cargo +1.95.0 run -p xtask -- arch` |

## Promotion rule

This spec is accepted when merged as the evidence source contract. It is
implemented when:

- a 0.21 implementation plan sequences adapters, adoption packs, status
  updates, external canaries, and closeout;
- generic command JSON import exists with fixture-backed unit, direction,
  sample, host, and error behavior;
- hyperfine JSON import exists with fixture-backed command timing semantics;
- Criterion import exists for stable mapped fields with clear non-inferences;
- pytest-benchmark JSON import exists with interpreter/environment limits;
- k6 summary JSON import exists with HTTP/load-test non-inferences;
- custom JSON/CSV mapping is explicit and fail-closed for ambiguous metrics;
- imported evidence reaches baseline doctor, signal doctor, policy doctor,
  policy review-packet, Action posture, and repair context where supported;
- adoption packs exist for Rust CLI, Rust workspace, Python service, Node
  tool/action, HTTP local smoke, and generic command repos;
- at least two external canaries prove the intake-to-review path across Rust
  and non-Rust repo shapes;
- product claims and canary freshness map adapter support without
  overclaiming; and
- a closeout records what teams can adopt without rewriting benchmarks, what
  remains advisory, what agents cannot change, and what remains unproven.
