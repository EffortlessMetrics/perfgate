//! Repo policy checkers.
//!
//! Three checkers live here:
//!
//! * [`no_panic`] — semantic panic-family allowlist (`policy/no-panic-allowlist.toml`).
//! * [`file_policy`] — non-Rust file allowlist (`policy/non-rust-allowlist.toml`).
//! * [`lint_policy`] — meta-check that workspace lints match `policy/clippy-lints.toml`
//!   and that every soft-staged lint has a `policy/clippy-debt.toml` receipt.
//!
//! All three are *advisory* by default during the rollout window: they emit
//! reports under `target/perfgate/reports/` and exit zero. Pass `--strict`
//! (or set `PERFGATE_POLICY_STRICT=1`) to make findings blocking.

pub mod common;
pub mod file_policy;
pub mod lint_policy;
pub mod no_panic;
