# Release Readiness

Last verified: 2026-05-14 for v0.17.0 publication reconciliation and the
0.18.0 release-candidate cutover proof. See
[v0.17.0 Publication Closeout](audits/release-0.17.0-publication-closeout.md),
[v0.17.0 Publish Readiness Proof](audits/release-0.17.0-publish-readiness.md),
[v0.18.0 Adoption Readiness Snapshot](audits/release-0.18.0-adoption-readiness.md),
[v0.18.0 Publish Readiness Proof](audits/release-0.18.0-publish-readiness.md),
[v0.18.0 Final Pre-Publish Proof](audits/release-0.18.0-final-prepublish-proof.md),
[v0.18.0 Publish Packet](audits/release-0.18.0-publish-packet.md),
and
[v0.18.0 Staged Release Artifact Smoke](audits/release-0.18.0-artifact-smoke.md).
The current 0.18 cutover lane remains active. The earlier
[v0.18.0 Deferral Closeout](audits/release-0.18.0-deferral-closeout.md)
is superseded because it correctly verified public state but incorrectly
archived the lane before publication. The earlier cutover decision is recorded
in [v0.18.0 Cutover Decision](audits/release-0.18.0-cutover-decision.md).

Latest published release: v0.17.0. The five allowed crates are published on
crates.io at `0.17.0`, the GitHub release `v0.17.0` is published with platform
archives and `sha256sums.txt`, and action alias tags `v0.17` and `v0` point to
the same release commit as `v0.17.0`.

## Current Published Release Snapshot

The current published release is `v0.17.0`. It keeps the five-crate public
surface from v0.16.0, raises the Rust floor to 1.95, and adds the governance
rails that make the release conveyor explicit.

The 0.18.0 release-candidate proof records are pre-release records, not
publication records. They document wrapper absorption, first-hour smoke proof,
structured-decision bundle proof, checked action failure examples, optional
server-ledger operations smoke, external canaries, publish dry-runs for all five
public crates, and staged Windows archive smoke. No public `0.18.0` crates,
tags, GitHub release, action aliases, or public install smoke exist yet. The
active release cutover lane has refreshed final pre-publish proof and is now
waiting at release-operator-gated publication. The operator packet is prepared,
but it does not authorize publication by itself.

## Current Publication State

| Surface | Status | Evidence |
|---------|--------|----------|
| crates.io packages | Published | The crates.io sparse index contains `0.17.0` entries for `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, and `perfgate-cli`. |
| Exact release tag | Published | GitHub tag `v0.17.0` points to commit `71bdc33117d515d95885deb2d9350d9d67905265`. |
| Action alias tags | Published | GitHub tags `v0.17` and `v0` also point to commit `71bdc33117d515d95885deb2d9350d9d67905265`. |
| GitHub release | Published | GitHub release `v0.17.0` was published on 2026-05-12 with platform archives and `sha256sums.txt`. |
| Public install smoke | Passing | `cargo +1.95.0 install perfgate-cli --version 0.17.0 --locked --root C:/perfgate-smoke/release-reconcile-0170 --force`; installed binary reported `perfgate 0.17.0` and `perfgate doctor --help` printed help. |

## Release Proof Matrix

| Gate | Status | Evidence |
|------|--------|----------|
| Rust 1.95 floor | Passing | `Cargo.toml`, `rust-toolchain.toml`, hosted workflow pins, and the composite action fallback toolchain use Rust 1.95 |
| Rust and Clippy policy | Passing | `clippy.toml`, `policy/clippy-lints.toml`, and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| No-panic governance | Passing | `cargo run -p xtask -- policy check-no-panic-family` and `policy/no-panic-baseline.toml` |
| Non-Rust file governance | Documented | `policy/non-rust-allowlist.toml`, companion allowlists, `docs/FILE_POLICY.md`, and `docs/POLICY_ALLOWLISTS.md` |
| CI evidence lane routing | Documented | `docs/ci/test-evidence-lanes.md`, with expensive fuzz, coverage, and self-dogfood lanes routed by label, `main`, schedule, or manual dispatch |
| Public package allowlist | Passing | `cargo run -p xtask -- public-surface --strict` |
| Architecture boundary enforcement | Passing | `cargo run -p xtask -- arch` |
| Publish metadata preflight | Passing | `cargo run -p xtask -- publish-check` |
| Package file-list proof | Passing | `cargo run -p xtask -- publish-check --package-list` |
| Adoption path docs | Covered | `README.md`, `docs/PERFORMANCE_DECISIONS.md` |
| Publish dry-run proof | Passing | `cargo run -p xtask -- publish-check --dry-run --package perfgate-types` |
| Publish dry-run matrix | Passing | `cargo run -p xtask -- publish-check --dry-run --package perfgate-types`, `cargo run -p xtask -- publish-check --dry-run --package perfgate`, `cargo run -p xtask -- publish-check --dry-run --package perfgate-client`, `cargo run -p xtask -- publish-check --dry-run --package perfgate-server`, `cargo run -p xtask -- publish-check --dry-run --package perfgate-cli` |
| GitHub Action install wiring | Passing | `cargo run -p xtask -- action-check` |
| Install smoke proof | Passing | Public registry install smoke passed with `cargo +1.95.0 install perfgate-cli --version 0.17.0 --locked --root C:/perfgate-smoke/release-reconcile-0170 --force`; installed binary reported `perfgate 0.17.0` and `perfgate doctor --help` printed help |
| Schema compatibility | Passing | `cargo run -p xtask -- schema-compat`, including `/health` response fixtures |
| Documentation examples | Passing | `cargo run -p xtask -- docs-check` and `cargo run -p xtask -- doc-test` |
| Structured decision end-to-end | Verified | `perfgate ingest probes`, `perfgate decision evaluate`, `perfgate decision bundle` on `examples/performance-decision`, plus `perfgate serve --no-open` and `decision upload/history/debt/prune --dry-run` |
| First-run paved road | Covered | `crates/perfgate-cli/tests/cli_first_run_e2e_tests.rs` |
| Baseline bootstrap UX | Covered | `crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs` |
| Structured decision workflow | Covered | `crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs`, `crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs`, `crates/perfgate-cli/tests/cli_release_decision_proof_tests.rs`, and GitHub Action `decision: "true"` |
| Decision ledger and debt | Covered | `decision upload|history|latest|export|prune|debt`, `perfgate.decision_record.v1`, decision upload/prune audit events, admin key create/list/rotate smoke, and dashboard decision-ledger tests |
| Signal-trust features | Covered | flakiness history, `baseline flaky`, inverse-variance aggregation, adaptive paired retries, local-regression caps, and noise-aware tradeoff review |
| Server operations visibility | Covered | `perfgate serve --doctor`, `/health`, `/metrics`, `audit list`, dashboard audit view tests, and dashboard decision-ledger tests |
| 0.18 adoption hardening | Covered | [v0.18.0 Adoption Readiness Snapshot](audits/release-0.18.0-adoption-readiness.md), including wrapper absorption, first-hour smoke, structured decision bundle proof, action summary examples, and server ledger operations smoke |
| 0.18 publish dry-run matrix | Passing | [v0.18.0 Publish Readiness Proof](audits/release-0.18.0-publish-readiness.md) packaged and verified `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, and `perfgate-cli` at `0.18.0` without uploading. |
| 0.18 final pre-publish proof | Passing | [v0.18.0 Final Pre-Publish Proof](audits/release-0.18.0-final-prepublish-proof.md) reran fmt, check, test, docs, source-doc, product-claim, public-surface, arch, action, schema, package-list, and five per-crate dry-run gates from the reopened release lane without publishing. |
| 0.18 publish packet | Prepared | [v0.18.0 Publish Packet](audits/release-0.18.0-publish-packet.md) records the release-operator command packet, publish order, stop conditions, partial-publish handling, and verification fields without publishing. |
| 0.18 staged artifact smoke | Passing | [v0.18.0 Staged Release Artifact Smoke](audits/release-0.18.0-artifact-smoke.md) unpacked a Windows release-like archive, verified `perfgate 0.18.0`, and ran zero-benchmark plus manual-benchmark first-hour smoke from the unpacked binary. |
| Full repo CI | Passing | Hosted `ci` passed on the release proof PR before publish; coverage, fuzz, and self-dogfood evidence remain routed by policy |

The only publishable packages allowed by policy are:

```text
perfgate
perfgate-cli
perfgate-types
perfgate-client
perfgate-server
```

Required release proof commands for the current 0.18.0 release candidate (run
without `--allow-dirty`):

```bash
cargo run -p xtask -- public-surface --strict
cargo run -p xtask -- arch
cargo run -p xtask -- publish-check --package-list
cargo run -p xtask -- publish-check --dry-run --package perfgate-types
cargo run -p xtask -- publish-check --dry-run --package perfgate
cargo run -p xtask -- publish-check --dry-run --package perfgate-client
cargo run -p xtask -- publish-check --dry-run --package perfgate-server
cargo run -p xtask -- publish-check --dry-run --package perfgate-cli
cargo run -p xtask -- action-check
cargo run -p xtask -- docs-check
cargo run -p xtask -- doc-test
cargo run -p xtask -- schema-compat
cargo run -p xtask -- ci
```

Run `publish-check --dry-run --package <name>` immediately before publishing each
crate in dependency order. Cargo verifies packaged dependencies against
crates.io, so downstream crates such as `perfgate` and `perfgate-cli` require
same-release workspace dependencies to be on the current path first.

For PR validation before the branch is committed or while release notes are still
being edited, `publish-check` also accepts `--allow-dirty`. Release operators
should omit it.

The GitHub release workflow builds platform archives,
unpacks each generated archive, verifies the binary exists, and runs
`perfgate --version` plus `perfgate doctor --help` on native targets before
uploading release assets.

The GitHub Action path is also guarded: `xtask action-check` verifies action
inputs and install wiring, and the action prints a local reproduction command
plus discovered artifact paths when `perfgate check` exits nonzero.

Former implementation packages such as `perfgate-domain` and `perfgate-app`
have been absorbed into the facade crate. Domain logic lives under
`perfgate::domain`; app orchestration and runtime adapters live under
`perfgate::app` and `perfgate::runtime`.

For the current baseline-service surface, see
[Baseline Service Notes](BASELINE_SERVICE_DESIGN.md) and
[Getting Started with the Baseline Server](GETTING_STARTED_BASELINE_SERVER.md).

For the current first-run path, see the `Start Here` section in
[the README](../README.md). The documented happy path is:

```bash
cargo binstall perfgate-cli
perfgate doctor
perfgate init --ci github --profile standard
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

For the structured-decision release proof, start from that generated setup and
prove the install-to-decision path. The first `check` creates a trusted first
run for promotion; the second `check --require-baseline` represents the next
change under review and writes compare receipts for `decision evaluate`:

```bash
cargo binstall perfgate-cli
perfgate doctor
perfgate init --ci github --profile standard
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
perfgate ingest probes --file artifacts/probes-baseline.jsonl --out artifacts/perfgate/parser/probes-baseline.json
perfgate ingest probes --file artifacts/probes.jsonl --out artifacts/perfgate/probes.json
perfgate decision evaluate --config perfgate.toml
```

Expected decision artifacts, under `[defaults].out_dir` unless overridden:

```text
artifacts/perfgate/
  <bench>/run.json
  <bench>/compare.json
  probes.json
  <bench>/probe-compare.json
  scenario.json
  tradeoff.json
  decision.md
  decision.index.json
  decision-bundle.json   # optional portable export from decision.index.json
```

`xtask action-check` guards the GitHub Action decision path: when
`decision: "true"` is enabled, the action runs `perfgate decision evaluate`,
lists `probe-compare.json`, `scenario.json`, `tradeoff.json`, `decision.md`,
and `decision.index.json` among discovered artifacts, and prints the local
`perfgate decision evaluate --config perfgate.toml` reproduction command. It
also guards `review_required: "warn" | "fail" | "pass"` so teams can choose
whether needs-review decisions emit a warning, block branch protection, or stay
advisory for downstream workflow policy.

## Historical Record: v0.15.1

## Patch Scope

This patch release is intentionally narrow:
- restore local `perfgate serve` baseline workflows (`promote --to-server`,
  `baseline list`, `baseline history`, `compare --baseline @server:<bench>`)
- align baseline-service docs with the actual `0.15.x` command and server
  surface
- roll examples and release docs forward to `v0.15.1`

## Current Status

- Workspace and internal crate versions are set to `0.15.1` on `main`.
- The local-mode baseline fix and doc cleanup are merged on `main`.
- `cargo run -p xtask -- ci` passed locally on 2026-03-28 against `main`.
- `cargo run -p xtask -- publish-check` passed locally on 2026-03-28 against `main`.
- GitHub release `v0.15.1` is published with platform binaries and `sha256sums.txt`.
- Publishable workspace crates are now published to crates.io at `0.15.1`.
- GitHub Action tags now include the exact release tag `v0.15.1` plus moving aliases `v0.15` and `v0`.

## Tested and Working

These commands were tested end-to-end on Windows (x86_64, Rust 1.92):

| Command | Status | Notes |
|---------|--------|-------|
| `run` | **Works** | Clean receipts, Windows IO metrics collected |
| `compare` | **Works** | Correct deltas, exit codes |
| `md` | **Works** | Clean Markdown table output |
| `github-annotations` | **Works** | Emits correct annotation format |
| `report` | **Works** | Generates perfgate.report.v1 |
| `promote` | **Works** | Copies and normalizes receipts |
| `export --format csv` | **Works** | Correct CSV with headers |
| `export --format jsonl` | **Works** | Valid JSON per line |
| `export --format junit` | **Works** | Valid XML |
| `export --format html` | **Works** | Valid HTML table |
| `export --format prometheus` | **Works** | Valid text exposition format |
| `check` | **Works** | Config-driven, finds baselines via `baseline_pattern` |
| `paired` | **Works** | Interleaved execution, produces compare receipt |
| `summary` | **Works** | Terminal table from compare receipts |
| `aggregate` | **Works** | Emits `perfgate.aggregate.v1` with policy verdicts |
| `explain` | **Works** | Generates diagnostic text |
| `blame` | **Works** | Diff two Cargo.lock files |
| `bisect` | **Not tested** | Wraps git bisect, requires repo with history |
| `serve` | **Works** | Local SQLite dashboard/server; local baseline workflows re-verified |
| `baseline upload` | **Works** | Requires `pg_live_` key with 32+ char suffix |
| `baseline list` | **Works** | Lists uploaded baselines correctly |
| `baseline download` | **Works** | Note: uses `--output` not `--out` |
| `baseline history` | **Works** | Local-mode smoke flow re-verified in 0.15.1 prep |
| `baseline delete/verdicts` | **Works** | Live server CLI workflow covers implicit-latest delete plus verdict submit/list |
| `check --mode cockpit` | **Works** | Produces sensor.report.v1 envelope + extras |

## Known Bugs

- [#55](https://github.com/EffortlessMetrics/perfgate/issues/55) ~~Leftover DEBUG prints~~ — **Fixed** (committed)
- [#56](https://github.com/EffortlessMetrics/perfgate/issues/56) ~~CLI examples in docs use wrong flags~~ — **Fixed** (committed)
- [#58](https://github.com/EffortlessMetrics/perfgate/issues/58) Server `--api-keys` glob `*` pattern causes 500 errors (use `.*` as workaround)

## Doc/Flag Mismatches Found During Testing (all fixed)

### `paired` command
- Docs said `--threshold 0.20` — **fixed** to `--fail-on-regression 20.0`
- Docs omitted required `--name` flag — **fixed**

### `blame` command
- Docs say `--compare cmp.json` — **wrong**. Actual: `--baseline <Cargo.lock> --current <Cargo.lock>`

### `bisect` command
- Docs say `--bench my-bench --config perfgate.toml` — **wrong**. Actual: `--good <COMMIT> --executable <PATH>`

### `baseline download` command
- Docs say `--out` — **wrong**, actual flag is `--output`

### `check` with `baseline_dir`
- `baseline_dir` may have path resolution issues in mixed Unix/Windows environments (MSYS2). `baseline_pattern` with absolute path works reliably.

### `run -p perfgate` vs `run -p perfgate-cli`
- `cargo run -p perfgate` fails (no bin target — it's a library facade). **Fixed**: all docs now use `cargo run -p perfgate-cli --`.

## What's Solid (ship with confidence)

The **core local gating pipeline** is production-quality:
- `run` → `compare` → `md`/`report` → `promote`
- `check` (config-driven single command)
- `paired` (noise-resistant benchmarking)
- All export formats
- Exit code contract (0/1/2/3)
- JSON receipt versioning
- Host fingerprinting
- Statistical significance (Welch's t-test)

## What's Functional But Needs Hardening

- **Baseline server** — works for dev/small-team. Storage backends (SQLite, PostgreSQL, S3) are implemented. Not load-tested. GitHub Actions OIDC is exercised; GitLab and custom OIDC exist but remain lightly exercised.
- **`bisect`** — wraps git bisect. Works in concept but depends on repo structure and build system. Edge cases likely.
- **`explain`** — generates prompts, doesn't call an LLM. Useful but the name oversells it.
- **`aggregate`** — formal fleet/matrix receipt with all/majority/weighted/quorum/fail-if-n-of-m gating. Weight keys use `os-arch` labels such as `linux-x86_64`.

## What's Still Missing / Deferred

- **Windows timeout support** — returns `AdapterError::TimeoutUnsupported`
- **Windows page_faults/ctx_switches** — not collected
- **OIDC beyond GitHub Actions** — GitLab/Okta not tested
- **`cargo run -p perfgate` ergonomics** — doesn't work without specifying `--bin`

## Added After v0.15.1 on Main

- **Paved first-run setup** — `perfgate init --ci github --profile standard`
  writes `perfgate.toml`, `.github/workflows/perfgate.yml`,
  `baselines/.gitkeep`, and `.perfgate/README.md`; `--preset` remains a
  compatibility alias.
- **Baseline bootstrap UX** — `perfgate baseline status` and
  `perfgate baseline promote --all` cover the local-baseline path without
  requiring users to hand-map receipt files.
- **Baseline server CLI coverage** — live server CLI workflows cover
  upload/list/download/history/run/compare/promote plus delete,
  submit-verdict, and verdict history across memory, SQLite, and PostgreSQL
  when `PERFGATE_TEST_POSTGRES_URL` is configured.
- **First-run e2e fixture** — the beginner flow now has an integration test
  that runs init, doctor, check, and baseline promotion against a generated
  project.
- **Action failure reproduction** — the composite action preserves artifacts
  and prints the local `perfgate check` command needed to reproduce a failed
  gate.
- **API key management CLI** — `admin keys create|list|revoke|rotate`
  manages server API keys through the CLI.
- **Audit logging** — baseline, verdict, and key mutations write audit events;
  `GET /api/v1/audit`, `perfgate audit list`, `perfgate audit export`, and the
  dashboard expose the audit log.
- **Server doctor and health detail** — `perfgate serve --doctor` preflights
  the local SQLite path, WAL setup, and dashboard port; `/health` reports
  sanitized storage detail and pool occupancy.
- **Operational metrics** — `/metrics` exposes request, auth, upload, verdict,
  baseline, and storage-error metrics with `perfgate_` names.
- **Executable doc tests** — `cargo run -p xtask -- doc-test` validates
  documentation CLI examples plus TOML, JSON, and YAML snippets, and runs from
  `xtask ci`.
- **Schema compatibility coverage** — `cargo run -p xtask -- schema-compat`
  checks historical receipt fixtures plus 0.16 baseline-service, audit,
  health, and fleet API fixtures.
- **Structured performance decisions** — `perfgate decision evaluate` consumes
  compare, scenario, probe-compare, and tradeoff receipts, writes
  `decision.md` plus `decision.index.json`, and is the taught advanced workflow
  for reviewable performance tradeoffs.
- **Portable decision bundles** — `perfgate decision bundle` exports
  `perfgate.decision_bundle.v1` from `decision.index.json` for release,
  audit, issue, or agent handoff attachment without requiring the server.
- **Probe evidence and tradeoff guardrails** — `perfgate ingest probes`,
  `perfgate probe compare`, scenario-attached probe evidence, probe-backed
  tradeoff requirements, local-regression caps, and noise-aware review policies
  are implemented and documented.
- **GitHub Action decision mode** — the action accepts `decision: "true"`,
  runs `perfgate decision evaluate`, uploads decision artifacts, and can defer a
  check policy failure to an accepted tradeoff receipt. Needs-review decisions
  are explicit through `review_required: "warn" | "fail" | "pass"`.
- **Decision ledger and debt** — the baseline server stores
  `perfgate.decision_record.v1` records, decision uploads emit audit events,
  `decision history|latest|export|prune|debt` expose the ledger from the CLI,
  prune operations require explicit `--dry-run` or `--force` and emit audit
  events when records are deleted, debt summaries include cap usage and
  accepted failed-metric deltas when receipt evidence is present, history can be
  filtered by scenario, status, verdict, review state, accepted-tradeoff
  presence, and rule, and the dashboard shows stored performance decisions with
  the same drilldowns.
- **Dashboard decision visibility** — the dashboard now includes baseline,
  verdict/flakiness, decision-ledger, and audit-event views.

## Post-Release Follow-Up

1. **Keep action runtimes current** — first-party actions are on current majors, but future runner/runtime upgrades still need periodic review
2. **Carry the release workflow repair forward** — future tags should continue using the recovered workflow now merged on `main`
3. **Keep crates publish preflight in CI** — `cargo run -p xtask -- publish-check` now guards missing crate readmes and `publish = false` workspace dependencies before release
