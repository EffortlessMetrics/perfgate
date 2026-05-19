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
| Optional server-ledger operations smoke | in-repo optional ledger operations | 2026-05-18 | [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md), [`2026-05-13-external-trust-closeout.md`](../handoffs/2026-05-13-external-trust-closeout.md), [`memory.rs`](../../crates/perfgate-server/src/storage/memory.rs) | In-repo proof covers optional decision upload/history/latest/debt/export, dry-run prune preservation, audit visibility, API key create/list/rotate smoke, and in-memory backup/restore equivalence for latest/history/audit plus prune dry-run preservation. | External team operation, production database backup/restore, production retention execution, large histories, migration compatibility, or any requirement that server mode be part of local correctness. | `current` |
| Probe-backed external canary | real repo with stable probe IDs | Not run | No durable artifact yet. | Nothing yet; this remains a planned canary shape for a repo with meaningful stable probes. | Probe adoption in external repos, probe naming stability under refactor, and probe-backed hosted CI. | `unproven` |
| Policy rollout canary | non-Rust command repo using advisory-to-review workflow | 2026-05-19 | [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md) | A real non-Rust repo can use benchmark suggestions, baseline doctor, signal doctor, policy doctor, non-mutating policy patch output, calibration patch output, and review packets to keep noisy evidence advisory. | Hosted external Action policy posture, public 0.20 install behavior, a mature `gate_candidate` promotion, probe-backed policy rollout, or server ledger correctness. | `current` |

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
