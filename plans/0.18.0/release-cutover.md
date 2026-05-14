# perfgate 0.18.0 Release Cutover Plan

Status: active
Owner: perfgate maintainers
Created: 2026-05-14
Milestone: 0.18.0
Current PR: release-operator-gated publication
Linked proposal: [`PERFGATE-PROP-0004-0-18-release-cutover`](../../docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)
Linked specs: [`PERFGATE-SPEC-0005-release-proof-contract`](../../docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../../docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../../docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Linked ADRs: [`PERFGATE-ADR-0001-public-crates-are-contracts`](../../docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md), [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../../docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked policy: [`public_crates.txt`](../../policy/public_crates.txt), [`absorbed_crates.txt`](../../policy/absorbed_crates.txt)
Support/status impact: [`PRODUCT_CLAIMS.md`](../../docs/status/PRODUCT_CLAIMS.md) and [`RELEASE_READINESS.md`](../../docs/RELEASE_READINESS.md) must match the public release state
Proof commands: docs-check; doc-test; docs-source-check; product-claims-check; public-surface --strict; arch; action-check; schema-compat; publish-check dry-runs; public install smoke after publication
Blocks: 0.18 publication closeout
Blocked by: explicit release-operator approval for crates.io publish, tags, GitHub release, and action alias movement
Rollback: before publication, revert the release-prep PRs; after publication, forward-fix crates/docs/tags and record repair notes because crates.io versions cannot be unpublished as a normal rollback

## Goal

Cut `0.18.0` with no ambiguity. A release operator should
be able to answer from repo artifacts:

```text
what version is public
which crates were published
which tags and action aliases moved
what public install smoke passed
what external canaries proved
what remains unproven
what should happen next
```

This plan sequences the release work. It does not authorize publishing crates,
moving tags, creating a GitHub release, or moving action aliases by itself.

## Operating Rules

- Keep one release semantic per PR: plan, version prep, dry-run proof, artifact
  smoke, docs cutover, publication, alias movement, public smoke, closeout.
- Do not publish crates from planning, docs, version-prep, or dry-run PRs.
- Do not create or move `v0.18.0`, `v0.18`, or `v0` from planning, docs,
  version-prep, or dry-run PRs.
- Move `v0` only after public install smoke proves 0.18.0 is the intended
  default action release.
- Preserve the five public crates:
  `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`,
  `perfgate-cli`.
- Keep local receipts as the correctness contract and server ledger mode as
  optional team history.
- Keep docs honest: readiness proof is not publication proof.

## PR Sequence

| PR | Work item | Status | Files / surface |
| --- | --- | --- | --- |
| 415 | Release cutover proposal | merged | `docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md` |
| 416 | Release cutover plan and active goal | merged | `plans/0.18.0/release-cutover.md`, `.codex/goals/active.toml` |
| 417 | Version prep | merged | workspace/package versions, changelog, release notes draft |
| 418 | Publish dry-run matrix | merged | `docs/audits/release-0.18.0-publish-readiness.md` |
| 419 | Release artifact smoke | merged | `docs/audits/release-0.18.0-artifact-smoke.md` |
| 420 | Public documentation cutover | merged | README, first-hour/adoption docs, release readiness, product claims |
| 421 | Premature deferral closeout | superseded | verified public state but incorrectly archived the lane |
| 422 | Reopen release lane | merged | `.codex/goals/active.toml`, release readiness, product claims, plan, superseded audit |
| 423 | Final pre-publish proof | implemented | `docs/audits/release-0.18.0-final-prepublish-proof.md` |
| 424 | Publish packet | implemented | `docs/audits/release-0.18.0-publish-packet.md` |
| 425 | Publish crates | blocked | crates.io publication in dependency order |
| 426 | Verify crates.io publication | blocked | `cargo info` / `cargo search` registry proof |
| 427 | Cut GitHub release | blocked | `v0.18.0`, GitHub release, release assets, checksums |
| 428 | Move `v0.18` alias | blocked | `v0.18` action alias |
| 429 | Decide and move `v0` default alias | blocked | `v0` action alias or explicit non-movement |
| 430 | Public install smoke | blocked | public path and first-hour smoke from published artifacts |
| 431 | Publication closeout | blocked | release closeout audit, product claims, archived goal |

## Work Item: version-prep

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Blocks: publish-dry-run-matrix
Blocked by:

### Goal

Prepare version and release-note state for 0.18.0 without publishing.

### Production delta

Expected files:

```text
Cargo.toml
crates/*/Cargo.toml if package versions are not inherited
CHANGELOG.md
docs/audits/release-0.18.0-notes.md or equivalent release notes draft
README.md or docs references only when they mention concrete versions
```

### Acceptance

- Workspace/package versions consistently point to 0.18.0.
- Release notes summarize actual merged changes: source-of-truth governance,
  guided adoption, wrapper absorption, external canaries, signal/probe/platform
  guidance, action failure examples, server-ledger key rotation smoke, and
  release cutover state.
- Docs do not claim 0.18.0 is published.

### Proof commands

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- public-surface --strict
git diff --check
```

### Rollback

Revert the version-prep PR before any crates are published.

## Work Item: publish-dry-run-matrix

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: release-artifact-smoke, publish-crates
Blocked by: version-prep

### Goal

Prove the publish graph without publishing.

### Acceptance

- Package list resolves to the five public crates.
- Per-package dry-runs pass in dependency order.
- The audit records command outputs, package order, commit, and any warnings.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- publish-check --package-list
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli
git diff --check
```

### Rollback

Revert the audit PR. No public state changes in this work item.

## Work Item: release-artifact-smoke

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: public-documentation-cutover, publish-crates
Blocked by: version-prep

### Goal

Prove the staged release artifact path before crates.io publication.

### Acceptance

- A release-like binary or archive reports `perfgate 0.18.0`.
- The staged artifact runs `doctor`, `init`, zero-benchmark guidance, manual
  benchmark check, baseline promotion, and required-baseline rerun.
- The audit clearly says the smoke was staged and not public install proof.

### Rollback

Revert the audit PR. No public state changes in this work item.

## Work Item: public-documentation-cutover

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked specs: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md; docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Blocks: public-install-smoke, publication-closeout
Blocked by: publish-dry-run-matrix, release-artifact-smoke

### Goal

Prepare public docs for release while distinguishing ready-to-release from
released.

### Acceptance

- README and user docs do not claim public 0.18.0 availability before publish.
- `RELEASE_READINESS.md` states the latest published release and the latest
  readiness proof.
- `PRODUCT_CLAIMS.md` links canary and release proof without overstating
  hosted external CI coverage.

### Rollback

Revert the docs PR before publication. After publication, forward-fix docs.

## Work Item: final-prepublish-proof

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: publish-crates
Blocked by:

### Goal

Refresh the full pre-publish proof from current `main` after the premature
deferral closeout is superseded.

### Acceptance

- Full workspace fmt, check, test, docs, source-doc, product-claim,
  public-surface, arch, action, schema, package-list, and per-crate dry-run
  gates pass from current `main`.
- `docs/audits/release-0.18.0-final-prepublish-proof.md` records the command
  set and non-inferences.

### Rollback

Revert the audit PR. No public state changes in this work item.

## Work Item: release-operator-gated-publication

Status: current
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: publish-crates, tag-release-aliases, public-install-smoke, publication-closeout
Blocked by: explicit release-operator approval

### Goal

Keep the lane active at the release-operator boundary. The next irreversible
steps are publishing crates, creating tags/releases/assets, moving action
aliases, and running public install smoke.

### Acceptance

- Prep remains recorded: 0.18.0 versions, publish dry-runs, staged artifact
  smoke, and public docs cutover.
- Latest public release remains `v0.17.0` until crates, tags, release assets,
  aliases, and public install smoke actually move.
- The lane is not archived until public install smoke and publication closeout
  are complete.

### Rollback

Before publication, revert only corrective or proof PRs as needed. After
publication, forward-fix public state and record repair notes.

## Work Item: publish-packet

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: publish-crates
Blocked by:

### Goal

Give the release operator a single copy-ready command packet for publishing the
five public crates without mutating public state in the packet PR.

### Acceptance

- The packet records the current proof basis, expected crate versions, publish
  order, exact publish commands, registry verification commands, stop
  conditions, and partial-publish handling.
- It explicitly says it does not authorize publication, create tags, create a
  GitHub release, move aliases, or prove public install.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the packet PR before publication. No public state changes in this work
item.

## Work Item: publish-crates

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: verify-crates-publication
Blocked by: explicit release-operator approval, publish-packet

### Goal

Publish the 0.18.0 crates in dependency order.

### Order

```text
perfgate-types
perfgate
perfgate-client
perfgate-server
perfgate-cli
```

### Acceptance

- Each crate exists on crates.io at 0.18.0.
- The publication audit records package URLs, timestamps, and operator notes.
- If any crate publish fails, later crates are not published and tag/release
  work remains blocked.

### Rollback

Crates.io publication is effectively forward-fix only. Repair with a patch
release and document the issue.

## Work Item: verify-crates-publication

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: cut-github-release
Blocked by: publish-crates

### Goal

Verify crates.io registry truth after publication and before any tag, release,
asset, or alias movement.

### Acceptance

- `cargo search perfgate-cli --limit 3` shows `0.18.0`.
- `cargo info` resolves all five crates.
- Same-release dependency versions point to `0.18.0` where expected.
- Release readiness still does not claim tag, asset, alias, or public install
  state.

### Proof commands

```bash
cargo +1.95.0 search perfgate-cli --limit 3
cargo +1.95.0 info perfgate-types
cargo +1.95.0 info perfgate
cargo +1.95.0 info perfgate-client
cargo +1.95.0 info perfgate-server
cargo +1.95.0 info perfgate-cli
```

### Rollback

If registry truth is incomplete, stop before tag/release/alias work and record a
repair audit.

## Work Item: cut-github-release

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: move-v0-18-alias
Blocked by: explicit release-operator approval, verify-crates-publication

### Goal

Create the exact `v0.18.0` release tag and GitHub release with assets and
checksums.

### Acceptance

- `v0.18.0` points to the intended release commit.
- GitHub release `v0.18.0` exists.
- Release assets exist and checksums are recorded.
- Asset downloads work.
- `v0.18` and `v0` remain unmoved unless explicitly handled in their own work
  items.

### Rollback

Move or repair release tags/assets only under explicit operator approval and
record repair notes.

## Work Item: move-v0-18-alias

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: move-v0-default-alias, public-install-smoke
Blocked by: explicit release-operator approval, cut-github-release

### Goal

Move the `v0.18` action alias to the `v0.18.0` release commit.

### Acceptance

- `v0.18.0` points to the release commit.
- `v0.18` points to the same release commit.
- Any alias-triggered workflows are expected, cancelled, completed, or recorded.

### Rollback

Move the alias again only under explicit operator approval and record repair
notes.

## Work Item: move-v0-default-alias

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Blocks: public-install-smoke
Blocked by: explicit release-operator approval, move-v0-18-alias

### Goal

Decide whether `v0` should move to the `v0.18.0` release commit. Move it only
if 0.18.0 is approved as the default action release.

### Acceptance

- If moved, `v0` points to the intended release commit and generated workflow
  examples are correct.
- If not moved, the audit records that `v0` remains on the prior release by
  decision and users should pin `v0.18` or `v0.18.0` for 0.18 behavior.
- Any alias-triggered workflows are expected, cancelled, completed, or recorded.

### Rollback

Move `v0` only under explicit operator approval and record repair notes.

## Work Item: public-install-smoke

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked specs: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md; docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Blocks: publication-closeout
Blocked by: verify-crates-publication, cut-github-release, move-v0-18-alias

### Goal

Prove a cold user can install and run the public 0.18.0 release.

### Acceptance

- Install uses public artifacts, not the workspace-built binary.
- Smoke covers `--version`, `doctor`, `init`, zero-benchmark guidance, manual
  benchmark, `check`, `baseline promote`, and `check --require-baseline`.
- Generated action workflow references the intended public action alias.

### Proof commands

```bash
cargo binstall perfgate-cli
perfgate --version
perfgate doctor
perfgate init --ci github --profile standard
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

### Rollback

If public install fails, do not hide it. Record failure, repair with a patch
release or docs correction, and update the publication closeout.

## Work Item: publication-closeout

Status: blocked
Linked proposal: docs/proposals/PERFGATE-PROP-0004-0-18-release-cutover.md
Linked specs: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md; docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Blocks:
Blocked by: public-install-smoke

### Goal

Close the lane with a durable public-state audit.

### Acceptance

- The closeout says what was published.
- It records crate URLs, tags, action aliases, GitHub release assets, public
  install smoke, canary evidence, product-claim updates, and non-inferences.
- `.codex/goals/active.toml` is archived with status `completed`.

### Rollback

Closeout audits should be superseded by a repair audit rather than edited to
erase prior public state.
