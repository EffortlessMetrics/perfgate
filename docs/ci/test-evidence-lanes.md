# CI Evidence Lanes

perfgate should keep ordinary PR verification fast enough to run consistently
while routing expensive evidence to labels, `main`, schedules, or explicit
release-proof work.

## Default PR Lane

Default PRs should prove the change is buildable, reviewable, and contract-safe:

```text
fmt
clippy
tests
docs-check
doc-test
schema-compat
public-surface
arch
action-check
no-panic policy
file policy
lint policy
```

Policy gates are added as their rollout PRs land. Until then, this document is
the routing target, not a claim that those commands exist today.

## Label, Main, Scheduled, or Release-Proof Lanes

Run heavier evidence when it buys signal:

| Lane | Routing |
|------|---------|
| Coverage | `main`, `workflow_dispatch`, or PR labels such as `coverage` and `full-ci`. |
| Fuzz | Scheduled, manual, release-proof, or high-risk parser/schema changes. |
| Baseline refresh | Dedicated refresh PRs, scheduled calibration, or explicit release proof. |
| Bench and trend refresh | Mainline/scheduled evidence lanes or labeled PRs. |
| Mutation testing | Dedicated policy lane if added later, not default PR traffic. |

This matches the existing coverage model in [coverage.md](coverage.md), where
coverage is execution-surface evidence and not a substitute for release,
schema, baseline, or mutation proof.

## Release Proof

Release proof should include the default PR lane plus package and schema proof:

```bash
cargo run -p xtask -- docs-check
cargo run -p xtask -- doc-test
cargo run -p xtask -- action-check
cargo run -p xtask -- public-surface --strict
cargo run -p xtask -- arch
cargo run -p xtask -- schema-compat
cargo run -p xtask -- publish-check --package-list
```

Dry-run publish proof must run per publishable crate in release order. Do not
hide expensive proof inside every ordinary PR to make a release checklist look
shorter.
