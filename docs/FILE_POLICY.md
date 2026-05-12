# File Policy

perfgate has important release and trust surfaces outside Rust source. The
Rust 1.95 governance rollout adds non-Rust file-surface policy so those files
are reviewed as contracts instead of incidental repository contents.

## Governed Surfaces

The initial allowlist should cover:

| Surface | Examples |
|---------|----------|
| GitHub workflows | `.github/workflows/*.yml` |
| Composite action | `action.yml` |
| Schemas | `schemas/*.json`, `contracts/schemas/*.json` |
| Fixtures | `fixtures/`, `contracts/fixtures/` |
| Docs | `README.md`, `docs/**/*.md` |
| Baselines and trends | `.ci/`, `baselines/`, trend artifacts |
| Coverage config | `codecov.yml` |
| Dependency policy | `deny.toml` |

## Required Fields

Every allowlist entry should include:

| Field | Purpose |
|-------|---------|
| `id` | Stable review identifier. |
| `glob` | File or file-family selector. |
| `kind` | Workflow, docs, schema, fixture, config, baseline, generated, or executable. |
| `language` | File format or language. |
| `surface` | User, CI, release, schema, dependency, or internal surface. |
| `classification` | Contract, generated, evidence, configuration, or documentation. |
| `owner` | Responsible reviewer or owner. |
| `reason` | Why the file is allowed and governed. |
| `covered_by` | Validation command, workflow, or review policy. |
| `created` | Date the entry was introduced. |
| `review_after` | Date or release for reevaluation. |

## Companion Ledgers

The planned policy files are:

- `policy/non-rust-allowlist.toml`
- `policy/generated-allowlist.toml`
- `policy/executable-allowlist.toml`
- `policy/workflow-allowlist.toml`
- `policy/dependency-surface-allowlist.toml`

The allowlists are contracts, not ignore files. New non-Rust files should either
fit an existing governed glob or add a reviewed entry with ownership and proof.

See [Policy Allowlists](POLICY_ALLOWLISTS.md) and
[Rust 1.95 and 0.17.0 Governance Rollout](development/RUST_1_95_ROLLOUT.md).
