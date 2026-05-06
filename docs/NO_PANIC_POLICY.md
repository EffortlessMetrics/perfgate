# No-Panic Policy

This document defines what "panic-free" means for the perfgate workspace and
how exceptions are receipted. The authoritative ledger is
[`policy/no-panic-allowlist.toml`](../policy/no-panic-allowlist.toml).

## Definition

> **No unreceipted panic-family behavior in production or tests.**

The panic-family covers all constructs that can abort the process or unwind
the stack outside a deliberately fallible Result path:

| Family | Examples |
|---|---|
| `unwrap` | `.unwrap()`, `Option::unwrap_or_panic()` |
| `expect` | `.expect("...")` |
| `panic_macro` | `panic!("...")` |
| `todo` | `todo!()`, `todo!("...")` |
| `unimplemented` | `unimplemented!()`, `unimplemented!("...")` |
| `unreachable` | `unreachable!()`, `unreachable!("...")` |
| `indexing` | `slice[i]`, `vec[i]` |
| `string_slice` | `&s[a..b]` on `str` |
| `get_unwrap` | `slice.get(i).unwrap()` |
| `unchecked_time_subtraction` | `Duration::sub`, `Instant::sub` without `checked_sub` |

The following families exist but are **not enabled** in v1:

| Family | Status |
|---|---|
| `assertion_macro` | Off by default; flips on once tests adopt fallible helpers (`ensure!`, `ensure_eq!`). `assert!` / `assert_eq!` remain valid test oracles. |
| `unwrap_unchecked` | Implies `unsafe`; covered by the unsafe-block discipline. |

## Why semantic, not Clippy-only

Clippy can deny `unwrap_used`, but Clippy cannot carry the *receipt* needed
for serious governance: owner, reason, classification, selector identity,
expiry, drift detection, and stale-entry detection.

The dual-rail design:

* **Rail A — Clippy** runs in editors and on every build. It catches the
  *shape* of panic-family code immediately. Existing call sites are staged at
  `warn` until receipted (see `policy/clippy-debt.toml`).
* **Rail B — semantic checker** (`cargo run -p xtask -- check-no-panic-family`)
  scans Rust source, matches against the allowlist by *path + family +
  selector*, and produces a policy report.

## Allowlist schema (v0.3)

```toml
schema_version = "0.3"

[[allow]]
id = "panic-0001"
path = "crates/perfgate-domain/src/stats.rs"
family = "unwrap"
classification = "domain_invariant"
owner = "perfgate-domain"
explanation = "Welch's t-test divides finite, validated variances; unwrap upholds invariant."
expires = "2026-09-30"

[allow.selector]
kind = "method_call"
container = "welch_t_test"
callee = "unwrap"
receiver_fingerprint = "self.variance.checked_div(n)"

# Advisory only — drift hint, never the matching key.
[allow.last_seen]
line = 142
column = 27
```

### Identity

```text
identity = path + family + selector
```

The checker matches entries by `path + family + selector`, never by
`(line, column)`. If a function moves within the file, the entry still
matches; if the surrounding code is rewritten enough that the selector no
longer matches, the entry is reported as **stale** and must be removed or
updated.

### Selector kinds

* `method_call` — `<receiver>.<callee>(<args>)`. Match by `container`
  (enclosing fn / item) + `callee` + a stable fingerprint of the receiver.
* `macro_call` — `panic!(...)`, `todo!(...)` etc. Match by `container` +
  macro name.
* `index` — `expr[idx]` or `expr[a..b]`. Match by `container` + index kind
  (`numeric`, `range`, `range_to`, `range_from`, `range_full`).

### Classifications

| Class | Meaning |
|---|---|
| `domain_invariant` | The unwrap upholds a checked invariant (post-validation). |
| `test_helper` | Fixture/setup boilerplate; migrate to fallible helper. |
| `proptest_invariant` | proptest assertion intentionally panics on failure. |
| `should_panic_oracle` | Test marked `#[should_panic]`; the panic IS the assertion. |
| `static_lookup` | `HashMap::get` over a compile-time-known key. |
| `infallible_parse` | `str::parse` / `Regex::new` over a vetted constant. |
| `tooling` | `xtask` / development scripts; not shipped. |
| `pending_burndown` | Acknowledged debt without classification; must have short expiry. |

### Required fields

| Field | Required | Notes |
|---|---|---|
| `id` | yes | Stable string; format `panic-NNNN`. Used in CI output. |
| `path` | yes | Path relative to workspace root. |
| `family` | yes | One of the family names above. |
| `classification` | yes | One of the categories above. |
| `owner` | yes | Crate or team handle. |
| `explanation` | yes | Single sentence; explain *why panic is acceptable here*. |
| `expires` | yes | ISO date. `pending_burndown` may not exceed 6 months. |
| `selector` | yes | Per the kinds above. |
| `last_seen` | no | Advisory; the checker auto-updates on next run. |

## Workflow

### Adding a new exception

```bash
# 1. Have the no-panic checker propose entries from current findings.
cargo run -p xtask -- no-panic propose

# 2. Open target/perfgate/reports/no-panic-proposed-allowlist.toml
#    Set `owner`, `classification`, `explanation`, and `expires` on each entry.

# 3. Move the entries you want to keep into policy/no-panic-allowlist.toml.

# 4. Re-run the checker; it should now exit clean.
cargo run -p xtask -- check-no-panic-family
```

### Removing an exception

When you remove the panic-family call site, also remove the matching entry.
`check-no-panic-family` fails on entries whose selector matches no current
finding — stale receipts are bugs.

### Drift

If the surrounding source moves, the checker auto-updates `last_seen` on its
next run and writes a diff to `target/perfgate/reports/no-panic-drift.md`.
Reviewers can sanity-check the new locations.

### Expiry

Every entry must have an `expires` date. Expired entries fail the checker.
Pre-merge bumping the date requires a justification line in the PR
description.

## Test-time policy (v1)

Tests follow the same rules as production code. The exception machinery is
the same allowlist. Two test-specific classifications make the common cases
ergonomic:

* `proptest_invariant` — `prop_assert!` / `prop_assert_eq!` and the local
  panics that proptest unwinds into shrinking signals.
* `should_panic_oracle` — `#[should_panic(expected = "...")]` tests where
  the panic itself is the oracle.

Test assertion macros (`assert!`, `assert_eq!`, `assert_ne!`) are *not*
considered part of the panic-family in v1. They are ordinary test oracles.
Promotion to `assertion_macro` is a v2 decision conditional on tests
returning `Result<()>` and using fallible helpers.

## Reports

`check-no-panic-family` writes two artifacts under
`target/perfgate/reports/`:

* `no-panic.md` — human-readable findings, grouped by crate.
* `no-panic.json` — machine-readable, suitable for CI annotations.

## Bootstrap

The first run of the checker treats the allowlist as advisory and writes a
proposed file. Once owners have reviewed and merged entries, the checker
flips to **blocking** — unallowlisted findings fail CI.

The blocking flip is staged per-crate (lowest churn first) so that one
crate's debt does not stall the whole workspace.
