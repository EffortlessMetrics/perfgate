# Canary Freshness Matrix

This matrix tracks external and release canary evidence by freshness. It keeps
canary proof useful without turning a single repo, runner, or smoke path into a
blanket product claim.

Freshness is not a support tier. Support tiers live in
[`SUPPORT_TIERS.md`](SUPPORT_TIERS.md), and product claims live in
[`PRODUCT_CLAIMS.md`](PRODUCT_CLAIMS.md). This file answers a narrower
question: which canary receipts are fresh enough to rely on for which kind of
adoption evidence?

## Freshness States

| State | Meaning |
|-------|---------|
| `current` | Proof applies to the current public release or current lane boundary. |
| `recent` | Proof is still relevant, but was not rerun from the latest public artifact or every current surface. |
| `stale` | Proof may still be informative, but should not support a new claim without rerun or newer corroboration. |
| `superseded` | Proof was replaced by newer evidence or an explicit closeout. |
| `unproven` | No durable proof artifact exists yet for this canary shape. |

## Matrix

| Canary | Repo shape | Last run | Proof artifact | What it proves | What it does not prove | Freshness |
|--------|------------|----------|----------------|----------------|------------------------|-----------|
| `diffguard` local first-hour canary | small Rust CLI workspace | 2026-05-13 | [`2026-05-13-external-canary-diffguard-small-rust-cli.md`](../audits/2026-05-13-external-canary-diffguard-small-rust-cli.md) | A small Rust CLI can reach local setup, check, promotion, required-baseline rerun, generated workflow wiring, and artifact output after a benchmark is configured. | Hosted CI, public `0.18.0` install, larger workspaces, non-Rust repos, probe-backed decisions, or server ledger operations. | `recent` |
| `shipper` local first-hour canary | larger Rust workspace | 2026-05-13 | [`2026-05-13-external-canary-shipper-large-rust-workspace.md`](../audits/2026-05-13-external-canary-shipper-large-rust-workspace.md) | A larger Rust workspace can use multiple command benches, multiple artifact directories, multiple promoted baselines, required-baseline rerun, and noisy-command guidance. | Hosted CI, non-Rust repos, public `0.18.0` install, probe-backed decisions, server ledger operations, or compile-heavy commands as good first-hour gates. | `recent` |
| `droid-action` local command canary | non-Rust TypeScript GitHub Action repo | 2026-05-13 | [`2026-05-13-external-canary-droid-action-non-rust-command.md`](../audits/2026-05-13-external-canary-droid-action-non-rust-command.md) | A non-Rust repo can use plain command benchmarks with the same config, artifact, baseline, workflow, and required-baseline model as Rust repos. | Hosted CI, public `0.18.0` install, probe-backed decisions, server ledger operations, or shell portability beyond the local Windows canary commands. | `recent` |
| `droid-action` hosted Action canary | hosted external PR on `ubuntu-24.04` | 2026-05-15 | [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md) | A non-perfgate repo PR can run the perfgate GitHub Action, upload artifacts on pass/fail paths, print local reproduction, and produce repair context after the summary shell fix. | Public `0.18.0`, `v0.18`, or `v0` alias behavior; every hosted runner; every repo shape; server ledger correctness; probe-backed external canaries. | `recent` |
| Public `0.18.0` install smoke | clean temporary repo from public release asset | 2026-05-18 | [`release-0.18.0-public-install-smoke.md`](../audits/release-0.18.0-public-install-smoke.md) | `cargo binstall perfgate-cli --version 0.18.0` can install the public binary, run doctor/init/check/promote/require-baseline, generate action wiring, and create expected artifacts. | Hosted external repository CI after publication, every platform archive by manual install, production threshold calibration, or server ledger correctness. | `current` |
| Action failure summary path | hosted external forced-failure canary plus checked examples | 2026-05-15 | [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md), [`action-failure-summaries.md`](../examples/action-failure-summaries.md) | A forced hosted failure prints verdict counts, artifact names, local reproduction, and repair context after the shell fix; in-repo examples guard common summary shapes. | Every shell/platform edge case, every action input combination, or public-release hosted canary rerun from `v0.18`. | `recent` |
| Action artifact upload path | hosted external pass/fail canary | 2026-05-15 | [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md) | Hosted pass and forced-failure action runs uploaded per-bench artifacts, including `run.json`, `compare.json`, `report.json`, and failure `repair_context.json`. | All artifact retention policies, all storage/download modes, or every runner platform. | `recent` |
| Hosted policy Action posture | hosted external PR on `ubuntu-24.04` | 2026-05-19 | [`2026-05-19-hosted-policy-action-canary-droid-action.md`](../audits/2026-05-19-hosted-policy-action-canary-droid-action.md) | A non-perfgate repo PR can run the current Action policy posture path, print advisory setup posture, include `policy doctor` and `policy review-packet` commands, preserve do-not guidance, and upload artifacts. | Public `0.20` install behavior, every hosted runner or input combination, mature `gate_candidate` promotion, `required_gate` approval, probe-backed policy rollout, or server ledger correctness. | `current` |
| Optional server-ledger operations smoke | in-repo optional ledger operations | 2026-05-18 | [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md), [`2026-05-13-external-trust-closeout.md`](../handoffs/2026-05-13-external-trust-closeout.md), [`memory.rs`](../../crates/perfgate-server/src/storage/memory.rs) | In-repo proof covers optional decision upload/history/latest/debt/export, dry-run prune preservation, audit visibility, API key create/list/rotate smoke, and in-memory backup/restore equivalence for latest/history/audit plus prune dry-run preservation. | External team operation, production database backup/restore, production retention execution, large histories, migration compatibility, or any requirement that server mode be part of local correctness. | `current` |
| Probe-backed external canary | real repo with stable probe IDs | Not run | No durable artifact yet. | Nothing yet; this remains a planned canary shape for a repo with meaningful stable probes. | Probe adoption in external repos, probe naming stability under refactor, and probe-backed hosted CI. | `unproven` |
| Policy rollout canary | non-Rust command repo using advisory-to-review workflow | 2026-05-19 | [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md) | A real non-Rust repo can use benchmark suggestions, baseline doctor, signal doctor, policy doctor, non-mutating policy patch output, calibration patch output, and review packets to keep noisy evidence advisory. | Hosted external Action policy posture, public 0.20 install behavior, a mature `gate_candidate` promotion, probe-backed policy rollout, or server ledger correctness. | `current` |
| Evidence intake adapter fixture proof | in-repo adapter fixture matrix | 2026-05-20 | [`cli_ingest_tests.rs`](../../crates/perfgate-cli/tests/cli_ingest_tests.rs), [`EVIDENCE_INTAKE.md`](../EVIDENCE_INTAKE.md), [`PERFGATE-SPEC-0013-evidence-source-contract`](../specs/PERFGATE-SPEC-0013-evidence-source-contract.md) | Generic command JSON, hyperfine, Criterion, pytest-benchmark, k6, custom JSON, and custom CSV adapters map source output into receipts with explicit unit, direction, sample model, host, and non-inference behavior. | External repo adoption, hosted Action import workflows, public release artifacts for 0.21 adapters, or every upstream tool JSON variant. | `current` |
| Imported evidence review-surface proof | in-repo maturity, policy, and Action fixtures | 2026-05-20 | [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs), [`cli_doctor_tests.rs`](../../crates/perfgate-cli/tests/cli_doctor_tests.rs), [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs), [`action.yml`](../../action.yml) | Imported evidence limits are visible in baseline doctor, signal doctor, calibration, policy doctor, review packets, and Action posture summaries where receipts expose source metadata. | Blocking policy promotion, hosted external 0.21 intake workflows, public release install behavior, or server-ledger correctness. | `current` |
| Adoption pack catalog proof | in-repo CLI and docs | 2026-05-20 | [`ADOPTION_PACKS.md`](../ADOPTION_PACKS.md), [`cli_adoption_tests.rs`](../../crates/perfgate-cli/tests/cli_adoption_tests.rs), [`adoption_packs.rs`](../../crates/perfgate-cli/src/adoption_packs.rs) | Rust CLI, Rust workspace, Python service, Node tool/action, HTTP local smoke, and generic command packs describe source, artifacts, local reproduction, Action posture, promotion path, bad fits, and non-inferences. | External repo adoption, public release proof for 0.21 adoption packs, or automatic benchmark selection/policy promotion. | `current` |
| 0.21 Rust intake canary | external Rust CLI repo using command evidence | 2026-05-20 | [`2026-05-20-evidence-intake-rust-canary-diffguard.md`](../audits/2026-05-20-evidence-intake-rust-canary-diffguard.md) | A real Rust CLI repo can import explicit generic command JSON into run receipts, promote a baseline, compare a later imported run, and review imported evidence through baseline doctor, signal doctor, policy doctor, and review packet output. | Public release artifacts for 0.21 adapters, hosted Action import workflows, Criterion/hyperfine external adoption, non-Rust intake, or proof that the smoke workload is a good PR gate. | `current` |
| 0.21 non-Rust intake canary | external TypeScript GitHub Action repo using command evidence | 2026-05-20 | [`2026-05-20-evidence-intake-non-rust-canary-droid-action.md`](../audits/2026-05-20-evidence-intake-non-rust-canary-droid-action.md) | A real non-Rust repo can import explicit generic command JSON into run receipts, promote a baseline, compare a later imported run, review imported evidence through baseline doctor, signal doctor, policy doctor, and review packet output, and generate local check repair context from the same command. | Public release artifacts for 0.21 adapters, hosted Action import workflows, HTTP/k6 proof, tool-specific pytest-benchmark adoption, shell portability beyond this Windows PowerShell wrapper, or proof that the smoke workload is a good PR gate. | `current` |
| First useful review fixture proof | in-repo first-use review loop | 2026-05-22 | [`cli_adoption_tests.rs`](../../crates/perfgate-cli/tests/cli_adoption_tests.rs), [`cli_review_tests.rs`](../../crates/perfgate-cli/tests/cli_review_tests.rs), [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs), [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs), [`action.yml`](../../action.yml) | The source-built CLI can recommend and dry-run adoption setup, explain review posture, surface benchmark passports, preserve agent guardrails, and emit non-mutating baseline/policy promotion plans. | Hosted external Action adoption, public release artifacts, every repo shape, every adapter, or proof that any suggested workload is a good blocking gate. | `current` |
| Hosted first-useful-review Action canary | external hosted PR using adoption, review explain, passport, repair, and promotion-plan surfaces | Not run | No durable artifact yet. | Nothing yet; this remains the hosted proof gap for the full 0.22 first-useful-review loop. | Hosted external Action behavior for the full review loop, artifact upload from this lane, fork safety, public release behavior, or cross-platform hosted runner behavior. | `unproven` |
| Public release first-useful-review canary | clean temporary repo from future public release artifact | Not run | No durable artifact yet. | Nothing yet; this remains the public-artifact proof gap for the full 0.22 first-useful-review loop. | Public install behavior for adoption recommend/apply, review explain, benchmark passport, repair context, or promotion plans. | `unproven` |

## Policy Rollout Rerun Plan

The 0.20 policy ergonomics lane adds advisory-to-blocking rollout surfaces on
top of the 0.19 evidence maturity work. Existing canaries remain useful
adoption history, but they did not exercise the full policy rollout path.

The next canary reruns should focus on whether a team can safely move from
advisory evidence toward reviewed policy without creating brittle gates.

| Target | Existing proof to reuse | Rerun should record | Do not infer |
|--------|-------------------------|---------------------|--------------|
| Small Rust CLI | `diffguard` local first-hour canary | benchmark recipe, advisory check, baseline doctor, signal doctor, promotion doctor, policy patch output, review packet | every Rust CLI workload is gate-ready |
| Large Rust workspace | `shipper` local first-hour canary | compile-heavy/advisory posture, noisy signal guidance, baseline maturity, promotion deferral, review packet | workspace test commands should block by default |
| Non-Rust command repo | `droid-action` local command canary | language-neutral recipe, advisory baseline, signal maturity, policy doctor, generated review packet | non-Rust shell portability across every runner |
| Hosted Action path | hosted `droid-action` Action canary | Action policy posture summary, artifact upload, local reproduction, advisory versus blocking wording | every hosted runner or action input combination |
| Public install path | `0.18.0` public install smoke | public install plus policy command discovery after a future public release | current-source canaries prove public artifacts |
| Failure summary path | hosted forced-failure canary plus examples | missing baseline, regression, maturity warning, and policy review-required wording | every shell/platform failure mode |
| Agent-heavy repo | no durable policy canary yet | review packet guardrails, allowed/review-required/forbidden actions, proof freshness handling | agents are policy authorities |

### Minimum Policy Canary Packet

Each policy rollout canary should record:

```text
repo shape
perfgate version or source commit
benchmark recipe or existing benchmark
baseline doctor output
signal doctor output
policy doctor output
policy emit-patch output
policy review-packet output
GitHub Action summary when hosted CI is part of the canary
what confused the user or agent
what changed in docs, config, or tooling
what it proves
what it does not prove
freshness state after the run
```

The first 0.20 policy canary only needs to cover one real repo, but it should
exercise the full advisory-to-promotion review path. Broader matrix reruns can
follow after the path is proven once.

### Rerun Boundaries

- Do not rerun every historical canary in this planning PR.
- Do not treat canaries as mandatory release gates without an accepted spec.
- Do not make a policy canary promote baselines, loosen thresholds, or make a
  gate blocking without a visible review surface.
- Do not cite source-built canaries as public-install proof.
- Keep server ledger mode optional team history in any canary that configures
  ledger upload.

## Evidence Intake Canary Plan

The 0.21 lane adds adapter and adoption-pack surfaces on top of the existing
evidence maturity and policy rollout paths. In-repo fixtures can support
adapter claims, but external canaries must prove that real repositories can keep
their existing benchmark tools while adopting perfgate review surfaces.

| Target | Existing proof to reuse | Rerun should record | Do not infer |
|--------|-------------------------|---------------------|--------------|
| Rust existing-benchmark repo | adapter fixture proof plus `diffguard`/`shipper` adoption history | source benchmark artifact, adapter command, imported run receipt, baseline doctor, signal doctor, policy doctor, review packet, optional Action posture | fixture success proves every Criterion or hyperfine repo |
| Non-Rust command or HTTP repo | adapter fixture proof plus `droid-action` command/policy canaries | source artifact, explicit field mapping or tool adapter, imported receipt, maturity output, policy output, review packet, optional hosted Action summary | one non-Rust repo proves every shell, runtime, or hosted runner |

Source-built canaries should be cited as current-source proof only. They do not
prove public release artifacts until a release installs and runs the same path.

## First Useful Review Canary Plan

The first useful performance review lane added the full source-built review
loop: adoption recommendation, dry-run setup, review explain, benchmark
passport, repair guardrails, and non-mutating promotion plans. The matrix keeps
that proof separate from hosted and public-artifact claims until those paths are
rerun deliberately.

| Target | Existing proof to reuse | Rerun should record | Do not infer |
|--------|-------------------------|---------------------|--------------|
| Hosted external Action review loop | first useful review fixture proof plus hosted policy Action canary | adoption recommendation output, dry-run setup artifacts, Action summary passport, uploaded artifacts, repair context, baseline promote-plan, policy promote-plan | every hosted runner, fork-safety path, or public release artifact behavior |
| Public release review loop | public install smoke plus first useful review fixture proof | public install source, adoption recommend/apply output, review explain output, benchmark passport, repair context, baseline/policy promote-plan output | current-source proof applies to public artifacts before the release that contains these commands |
| Existing benchmark-tool repo | evidence intake adapter canaries plus first useful review fixture proof | source benchmark artifact, adapter command, imported receipt, review explain, benchmark passport, promotion plans, non-inferences encountered | one tool adapter proves every upstream JSON variant or every workload is gate-ready |
| Agent repair handoff | repair-context fixtures plus review explain guardrails | repair context path, allowed commands, forbidden changes, human-review requirements, proof commands after repair, agent confusion or unsafe suggestion avoided | agents are policy authorities or may promote baselines, loosen thresholds, accept tradeoffs, or require ledger mode |

### Minimum First Useful Review Canary Packet

Each first-useful-review canary should record:

```text
repo shape
perfgate version, source commit, or public artifact
adoption recommendation output
dry-run setup artifacts
evidence source and receipt path
review explain output
benchmark passport
repair context path when generated
baseline promote-plan output
policy promote-plan output
Action summary and uploaded artifacts when hosted CI is part of the canary
what confused the user, reviewer, or agent
what changed in docs, config, or tooling
what it proves
what it does not prove
freshness state after the run
```

### First Useful Review Boundaries

- Do not cite source-built first-useful-review proof as hosted Action proof.
- Do not cite hosted Action proof as public-release proof.
- Do not make a canary promote baselines, loosen thresholds, or make a gate
  blocking without a visible review surface.
- Do not treat adoption recommendation as proof that a workload is a good
  required gate.
- Keep repair context and agent prompts advisory; agents remain reviewers and
  fixers, not policy authorities.

## Use

Use this matrix when updating product claims, release notes, or closeouts:

- Cite `current` canaries for current release claims.
- Cite `recent` canaries only with their stated limits.
- Do not cite `stale` canaries as standalone proof for a new claim.
- Treat `superseded` canaries as history unless the newer proof links back to
  them.
- Leave `unproven` canaries visible so the gap is explicit.

## Refresh Triggers

Refresh or rerun a canary when:

- action summary behavior changes;
- generated workflow defaults change;
- artifact layout changes;
- first-hour init/check/promote guidance changes;
- public release aliases move;
- server ledger operations change; or
- a product claim wants stronger proof than the matrix currently supports.

## Non-Inferences

- One hosted external PR does not prove every external repository or runner.
- Local canaries do not prove hosted CI.
- Public install smoke does not prove every external repo adoption path.
- Server ledger proof does not make server ledger mode required for correctness.
- Canary freshness does not replace product claim support tiers.
