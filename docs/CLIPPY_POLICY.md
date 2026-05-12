# Clippy Policy

perfgate treats Clippy as a staged policy surface. The current workspace policy
is intentionally light: `all = "warn"` at workspace level, with CI invoking
Clippy under `-D warnings`.

The Rust 1.95 rollout moves this to an explicit ledger model without enabling
blanket categories by accident.

## Target Files

| File | Role |
|------|------|
| `clippy.toml` | Tool-level MSRV declaration, starting with `msrv = "1.95"`. |
| `policy/clippy-lints.toml` | Active and planned lint ledger. |
| `policy/clippy-debt.toml` | Known debt that is not yet ratcheted. |
| `policy/clippy-exceptions.toml` | Narrow exceptions with owners, reasons, and review dates. |

## Policy Defaults

| Setting | Target |
|---------|--------|
| MSRV | `1.95` |
| Panic-free tests | true |
| Test carveouts | false by default |
| Suppressions | `expect` or allow with a reason |
| Blanket categories | false |

Initial active lints should be small and reviewable:

| Lint | Level | Reason |
|------|-------|--------|
| `clippy::dbg_macro` | deny | Debug macros are not reviewable diagnostics. |
| `clippy::todo` | deny | TODO execution paths are not allowed. |
| `clippy::unimplemented` | deny | Unimplemented execution paths are not allowed. |

Rust 1.95 ratchets must be measured before activation:

| Lint | Candidate Level |
|------|-----------------|
| `clippy::same_length_and_capacity` | deny |
| `clippy::manual_checked_ops` | warn |
| `clippy::manual_take` | warn |
| `clippy::manual_pop_if` | warn |
| `clippy::duration_suboptimal_units` | warn |

Because CI uses `-D warnings`, warning-level ratchets become hard failures in
practice. Do not add noisy warning lints unless the PR also fixes the warnings.

## Rollout Rules

1. Add the ledger before enforcement.
2. Measure each candidate lint against the workspace.
3. Activate only clean or cheap lints in the ratchet PR.
4. Keep `disallowed_fields` out until protected seams are real.
5. Keep every exception scoped to a selector, owner, reason, and review date.

See [Rust 1.95 and 0.17.0 Governance Rollout](development/RUST_1_95_ROLLOUT.md).
