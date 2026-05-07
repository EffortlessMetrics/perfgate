# Clippy / Rust Lint Policy

This document describes the lint policy for the perfgate workspace. It is the
narrative companion to [`policy/clippy-lints.toml`](../policy/clippy-lints.toml)
(the authoritative declaration) and [`policy/clippy-debt.toml`](../policy/clippy-debt.toml)
(the receipted ledger of debt-staged lints).

## Principles

> **Deny by default. Allow by receipt. Expire exceptions. Measure drift.**

The policy is implemented as a *stack*, not a single Clippy invocation:

```text
Clippy lints
  catch local bad Rust shapes
↓
semantic no-panic checker
  owns location + reason allowlists for panic-family debt
↓
non-Rust file policy checker
  owns location + reason allowlists for non-Rust surfaces
↓
lint-policy checker
  verifies every crate inherits the shared rules
```

Each layer has a different job:

* **Clippy** runs on every `cargo build` / `cargo clippy` and is what
  developers see in their editors. It catches local code shapes.
* The **semantic no-panic checker** owns *intent*: it maps panic-family call
  sites to owners, reasons, and expiries.
* The **file-policy checker** owns the boundaries of what counts as
  implementation language vs. ancillary surfaces.
* The **lint-policy checker** is a meta-check: it makes sure the workspace
  declaration matches `policy/clippy-lints.toml` and that every debt entry
  expires.

## Where the policy lives

| File | Role |
|---|---|
| `policy/clippy-lints.toml` | Authoritative list of active and planned lints |
| `policy/clippy-debt.toml` | Receipts for lints staged softer than their target level |
| `Cargo.toml` `[workspace.lints.*]` | Materialized form Cargo consumes |
| `clippy.toml` | (Reserved) Per-lint config; **no test carveouts allowed** |

## Active rules (summary)

The full enumeration with reasons lives in `policy/clippy-lints.toml`. The
categories are:

* **Panic-family / abrupt exits** — `unwrap_used`, `expect_used`, `panic`,
  `todo`, `unimplemented`, `unreachable`, `dbg_macro`, `get_unwrap`,
  `unwrap_in_result`, `string_slice`, `indexing_slicing`,
  `out_of_bounds_indexing`, `unchecked_duration_subtraction`.
* **UTF-8 / parser safety** — `char_indices_as_byte_indices`,
  `index_refutable_slice`.
* **Silent failure** — `let_underscore_future`, `let_underscore_must_use`,
  `let_underscore_lock`, `map_err_ignore`, `assertions_on_result_states`,
  `lines_filter_map_ok`.
* **Async / concurrency** — `await_holding_lock`, `await_holding_refcell_ref`,
  `await_holding_invalid_type`, `future_not_send`, `arc_with_non_send_sync`,
  `rc_mutex`, `mut_mutex_lock`, `readonly_write_lock`.
* **Unsafe / memory** — `mem_forget`, `forget_non_drop`, `drop_non_drop`,
  `undocumented_unsafe_blocks`, `multiple_unsafe_ops_per_block`,
  `unsafe_op_in_unsafe_fn`.
* **Numeric correctness** — `float_cmp`, `float_cmp_const`,
  `float_equality_without_abs`, `lossy_float_literal`, `cast_sign_loss`,
  `cast_possible_wrap`, `cast_possible_truncation`, `cast_precision_loss`,
  `invalid_upcast_comparisons`, `cast_abs_to_unsigned`, `cast_enum_truncation`,
  `cast_nan_to_int`.
* **File / process / path** — `suspicious_open_options`,
  `nonsensical_open_options`, `ineffective_open_options`,
  `path_buf_push_overwrite`, `join_absolute_paths`, `read_line_without_trim`,
  `exit`.
* **API correctness** — `iter_not_returning_iterator`,
  `expl_impl_clone_on_copy`, `infallible_try_from`, `fallible_impl_from`,
  `error_impl_error`, `result_unit_err`, `result_large_err`.
* **Format / good taste** — `format_in_format_args`,
  `to_string_in_format_args`, `unused_format_specs`, `uninlined_format_args`,
  `manual_let_else`, `manual_ok_or`, `manual_strip`, `manual_split_once`,
  `filter_map_next`, `flat_map_option`, `match_result_ok`, `needless_collect`.
* **Documentation discipline** — `missing_panics_doc`, `missing_errors_doc`.
* **Suppression governance** — `allow_attributes`,
  `allow_attributes_without_reason`, `blanket_clippy_restriction_lints`,
  `should_panic_without_expect`.

## Suppression rules

* **No bare `#[allow(...)]`.** Use `#[expect(..., reason = "...")]`.
* **No category profiles.** `#![warn(clippy::pedantic)]` or
  `#![warn(clippy::restriction)]` are forbidden — opt-ins must be deliberate.
* **No test carveouts in `clippy.toml`.** None of these are permitted:
  ```toml
  allow-unwrap-in-tests = true
  allow-expect-in-tests = true
  allow-panic-in-tests  = true
  allow-indexing-slicing-in-tests = true
  allow-dbg-in-tests   = true
  ```
  Tests must follow the same panic-family rules as production code, with
  exceptions receipted in `policy/no-panic-allowlist.toml`.

### When you need an exception

1. **Code-shape suppression** — write `#[expect(clippy::lint_name, reason = "<one line>")]`
   directly above the construct. The reason should be specific (a tracking
   issue, an invariant the suppression upholds), not "false positive".
2. **Panic-family exception** — add an entry to
   `policy/no-panic-allowlist.toml` with `owner`, `reason`, `classification`,
   selector, and `expires`. See [NO_PANIC_POLICY.md](NO_PANIC_POLICY.md).
3. **Lint not yet at target level** — add an entry to
   `policy/clippy-debt.toml` with `owner`, `reason`, and `expires`. The
   lint-policy checker fails on stale debt.

## Suppression style

```rust
// Correct
#[expect(
    clippy::cast_precision_loss,
    reason = "Statistics intentionally accept i64→f64 narrowing; values are bounded.",
)]
let mean = (sum as f64) / (n as f64);

// Wrong: bare allow
#[allow(clippy::cast_precision_loss)]   // forbidden
let mean = (sum as f64) / (n as f64);

// Wrong: no reason
#[expect(clippy::cast_precision_loss)]  // forbidden — reason required
```

## Burndown ledger (`clippy-debt.toml`)

The workspace `[lints.*]` block declares the **policy target** (e.g.
`unwrap_used = "warn"`). Existing findings that would fail
`cargo clippy ... -- -D warnings` today are receipted in
[`policy/clippy-debt.toml`](../policy/clippy-debt.toml). Each entry has:

```toml
[[debt]]
lint            = "clippy::unwrap_used"
current_level   = "allow"   # the level CI overlays via `-A clippy::<name>`
target_level    = "warn"    # what the policy actually wants
owner           = "perfgate-core"
reason          = "~2.5k existing call sites; receipt via no-panic allowlist."
expires         = "2026-12-31"
```

`xtask ci` reads the ledger and appends `-A clippy::<lint>` for every entry
with `current_level = "allow"`. Editors and `cargo clippy` (without xtask)
still surface the warnings — only the CI gate is muted. When an entry is
removed from the ledger, the lint immediately starts failing CI. **That is
the ratchet.**

### Promotion path

1. Pick a lint with a debt entry.
2. Burn down the findings in source (or, for panic-family, populate
   `policy/no-panic-allowlist.toml` via
   `cargo run -p xtask -- no-panic propose`).
3. Verify locally: temporarily comment out the debt entry and run
   `cargo run -p xtask -- ci`.
4. When clean, delete the debt entry. The lint becomes blocking on the next
   CI run.

The lint-policy checker fails if:

* a debt entry's `target_level` is softer than the policy's declared level
* a debt entry's `current_level` is *stricter* than the workspace level
  (debt should mute, not promote)
* the workspace level has caught up to `target_level` and the entry is
  no longer needed (stale debt)
* an `expires` date has passed

## Planned 1.94 / 1.95 promotions

`policy/clippy-lints.toml` carries `[[planned]]` entries for lints that will
activate when MSRV bumps. The lint-policy checker fails if a planned lint is
*active* before its target Rust version (you'd be silently on nightly).

## Disallowed methods/macros

`policy/clippy-lints.toml` lists `[[disallowed_method]]` and (when added)
`[[disallowed_macro]]` entries. These are mirrored to `clippy.toml` once the
`disallowed_methods` / `disallowed_macros` lints are enabled. Until then they
are documentation-as-code: contributors should treat them as binding intent
even though Clippy will not yet enforce them.

## CI integration

The policy stack runs as part of `cargo run -p xtask -- ci`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p xtask -- check-lint-policy
cargo run -p xtask -- check-no-panic-family
cargo run -p xtask -- check-file-policy
```

The three policy checks run as **advisory** during the rollout window — they
emit reports under `target/perfgate/reports/` but do not fail CI. They become
blocking once the corresponding allowlists are baselined and reviewed.

## Repo-class

perfgate is classified as **pure Rust library + CLI + service** with one
unsafe island (Unix `wait4()` in `perfgate-adapters`). Therefore:

```text
unsafe_code = "deny"          (not "forbid"; receipted unsafe islands exist)
unsafe_op_in_unsafe_fn = "deny"
undocumented_unsafe_blocks = "deny"
multiple_unsafe_ops_per_block = "deny"
```
