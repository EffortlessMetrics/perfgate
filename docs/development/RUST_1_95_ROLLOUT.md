# Rust 1.95 and 0.17.0 Governance Rollout

This is the control map for the perfgate 0.17.0 trust/governance release.
The 0.16.0 line delivered the structured-decision product surface.
The 0.17.0 line makes the implementation and release conveyor governed.

The MSRV increase makes this a minor release. Do not ship the Rust 1.95 floor
as a 0.16.x patch.

Release themes:

- Rust 1.95 floor
- staged Clippy policy
- panic-family governance
- non-Rust file-surface governance
- release-order publish proof
- schema, action, docs, and public-surface gates
- explicit CI evidence-lane routing

## Scope Guard

The first PR in this rollout is documentation-only.
It must not change `Cargo.toml`, `rust-toolchain.toml`, workflows, or Rust source.

Do not continue any partial local no-panic implementation in this roadmap PR.
That work belongs in a planned no-panic policy lane after the MSRV and lint
policy foundation is explicit.

This rollout is separate from baseline or trend refresh work such as #334.

## Current and Target State

| Layer | Current | Target |
|-------|---------|--------|
| Edition | 2024 | 2024 |
| Version | 0.16.0 | 0.17.0 |
| MSRV | 1.95 | 1.95 |
| Toolchain | 1.95.0 | 1.95.0 |
| Hosted Rust workflow pins | 1.95.0 | 1.95.0 |
| `clippy.toml` | present, `msrv = "1.95"` | present, `msrv = "1.95"` |
| Rust lints | `unsafe_code = "warn"` | explicit 1.95 floor |
| Clippy policy | light | staged ledger/checker |
| No-panic policy | absent | no-new-debt baseline/allowlist |
| Non-Rust file policy | absent | allowlist plus companion ledgers |
| Public crate surface | five crates | keep enforced |
| Release proof | working | keep publish matrix boring |
| CI economics | basic routed lanes | explicit evidence lane policy |

## PR Ladder

1. `docs(policy): map Rust 1.95 and 0.17.0 governance rollout`
   - Documentation-only roadmap, policy targets, release-readiness truth, and changelog note.
2. `chore(msrv): probe Rust 1.95 compatibility`
   - Run current `main` under Rust 1.95 and record the audit before support changes.
3. `chore(msrv): raise workspace toolchain to Rust 1.95`
   - Bump MSRV, toolchain, CI pins, coverage pins, and `clippy.toml`; do not bump versions.
4. `policy(rust): enable Rust 1.95 compiler lint floor`
   - Add low-noise Rust compiler lints while keeping `unsafe_code = "warn"`.
5. `policy(clippy): add staged lint policy ledger`
   - Add lint, debt, and exception ledgers; enforcement can follow if it is not small.
6. `policy(clippy): activate Rust 1.95 lint ratchets`
   - Measure first, then activate only clean or cheap Rust 1.95 Clippy ratchets.
7. `policy(panic): add exact no-panic policy`
   - Add exact panic-family policy and scanner using counted identities.
8. `policy(panic): add no-new-debt baseline`
   - Add the generated baseline and forbid silent new-debt absorption.
9. `policy(files): add non-Rust file allowlist`
   - Add non-Rust, generated, executable, workflow, and dependency-surface allowlists.
10. `ci: document and route evidence lanes`
   - Keep heavy evidence out of ordinary PRs unless explicitly routed.
11. `refactor: use Rust 1.95 APIs in decision and policy builders`
   - Apply targeted cleanup only where it reduces risk or noise.
12. `release: prepare 0.17.0 for Rust 1.95`
   - Bump versions, docs, examples, internal dependencies, and changelog.
13. `release: validate 0.17.0 publish readiness`
   - Run release proof, publish dry-runs in dependency order, and tag only after `main` is green.

## Acceptance Gates

The roadmap PR must pass:

```bash
cargo run -p xtask -- docs-check
cargo run -p xtask -- doc-test
cargo run -p xtask -- action-check
cargo run -p xtask -- public-surface --strict
cargo run -p xtask -- arch
git diff --check
```

The release-proof PR must additionally include:

```bash
cargo run -p xtask -- schema-compat
cargo run -p xtask -- publish-check --package-list
cargo run -p xtask -- publish-check --dry-run --package perfgate-types
cargo run -p xtask -- publish-check --dry-run --package perfgate
cargo run -p xtask -- publish-check --dry-run --package perfgate-client
cargo run -p xtask -- publish-check --dry-run --package perfgate-server
cargo run -p xtask -- publish-check --dry-run --package perfgate-cli
```

The dry-run order must stay boring: package each public crate only after its
same-release workspace dependencies are available through the current release
path.

## Policy Targets

### Clippy

The target is a staged ledger, not blanket category activation. Initial active
lints should be explicit, low-noise lints such as `clippy::dbg_macro`,
`clippy::todo`, and `clippy::unimplemented`.
Planned Rust 1.95 ratchets include `clippy::same_length_and_capacity`,
`clippy::manual_checked_ops`, `clippy::manual_take`,
`clippy::manual_pop_if`, and `clippy::duration_suboptimal_units` only after
measurement.

Warnings are effectively hard failures when CI runs Clippy with `-D warnings`,
so warning-level additions must be clean or cleaned in the same PR.

### No-Panic Family

The policy must use exact counted identities from the start:

| Field | Purpose |
|-------|---------|
| `path` | Source path containing the callsite. |
| `family` | Panic-family group. |
| `selector_kind` | How the callsite was matched. |
| `selector_callee` | Exact macro or method selector. |
| `snippet` | Stable local source snippet. |
| `count` | Expected count for that identity. |
| `owner` | Review owner for the allowance. |
| `reason` | Why the debt exists or is acceptable. |
| `review_after` | Date or release when the allowance must be revisited. |

Do not key only by path and panic family. Baseline refreshes may drop
disappeared debt, but must not silently absorb new debt.

### Non-Rust File Surface

Every governed non-Rust surface should carry:

```text
id
glob
kind
language
surface
classification
owner
reason
covered_by
created
review_after
```

Important surfaces include GitHub workflows, the composite `action.yml`, schema
JSON, fixtures, docs, baselines and trends, Codecov config, and `deny.toml`.

### CI Evidence Lanes

Default PRs should stay fast and reviewable:

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
no-panic/file/lint policy
```

Coverage, fuzzing, baseline refreshes, heavier benchmark and trend lanes, and
mutation testing belong on labels, `main`, schedules, or explicit release-proof
work where their cost buys signal.

## References

- [Clippy Policy](../CLIPPY_POLICY.md)
- [No-Panic Policy](../NO_PANIC_POLICY.md)
- [File Policy](../FILE_POLICY.md)
- [Policy Allowlists](../POLICY_ALLOWLISTS.md)
- [CI Evidence Lanes](../ci/test-evidence-lanes.md)
- [Release Readiness](../RELEASE_READINESS.md)
