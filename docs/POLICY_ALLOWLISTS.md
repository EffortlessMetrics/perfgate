# Policy Allowlists

Policy allowlists describe intentional repository surface. They are narrow,
owned ledgers that let automation distinguish accepted contracts from accidental
drift.

## Existing Policy Files

| File | Purpose |
|------|---------|
| `policy/public_crates.txt` | Public publishable crate surface. |
| `policy/absorbed_crates.txt` | Workspace-only compatibility and absorbed crate dispositions. |

The public crate policy currently allows only:

```text
perfgate
perfgate-cli
perfgate-types
perfgate-client
perfgate-server
```

This five-crate surface must stay enforced by
`cargo run -p xtask -- public-surface --strict`.

## Planned Policy Files

| File | Purpose |
|------|---------|
| `policy/clippy-lints.toml` | Active and planned Clippy lint policy. |
| `policy/clippy-debt.toml` | Known Clippy debt not yet ratcheted. |
| `policy/clippy-exceptions.toml` | Narrow Clippy exceptions. |
| `policy/no-panic-allowlist.toml` | Intentional panic-family callsites. |
| `policy/no-panic-baseline.toml` | Generated no-new-debt panic-family baseline. |
| `policy/non-rust-allowlist.toml` | Governed non-Rust file surface. |
| `policy/generated-allowlist.toml` | Generated files and refresh rules. |
| `policy/executable-allowlist.toml` | Executable files and permission expectations. |
| `policy/workflow-allowlist.toml` | Workflow files and CI ownership. |
| `policy/dependency-surface-allowlist.toml` | Dependency-policy and lockfile surfaces. |

## Review Rules

1. Every allowance needs an owner, reason, and review date.
2. Generated baselines may remove disappeared debt but must not absorb new debt.
3. Exceptions must be selector-scoped where possible.
4. Release PRs must report which policy gates were run.
5. Policy files must change in the same PR as the governed surface change.

See [Clippy Policy](CLIPPY_POLICY.md), [No-Panic Policy](NO_PANIC_POLICY.md),
and [File Policy](FILE_POLICY.md).
