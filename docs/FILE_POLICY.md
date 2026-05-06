# Non-Rust File Policy

This document describes the per-repo non-Rust file policy and how it is
enforced. The authoritative ledger is
[`policy/non-rust-allowlist.toml`](../policy/non-rust-allowlist.toml).

## Principle

> **Default implementation path is Rust + xtask. Non-Rust source/config
> surfaces require an explicit, owned, reason-receipted entry.**

This is not anti-other-languages — it is anti-undeclared. Every non-Rust file
must have an owner, a reason, and (where applicable) a covered-by check.

## What "non-Rust" means

The checker classifies files by extension and path. Files that are *implicitly
Rust-adjacent* (manifests, lockfiles, the toolchain pin) are not flagged. The
checker focuses on:

* Programming language source other than Rust (TS, JS, Python, shell, etc.)
* Static configuration files (YAML, TOML at non-Cargo paths, JSON, INI)
* Schema / contract files (JSON Schema, OpenAPI, protobuf)
* Documentation (Markdown, ADRs, diagrams)
* Generated artifacts (badges, dashboards)
* Fixtures / corpora / golden files

## Schema (v1.0)

```toml
schema_version = "1.0"

[[allow]]
glob = "fixtures/**/*.json"
kind = "fixture_input"
owner = "perfgate-core"
surface = "fixtures"
classification = "test"
reason = "Schema and conformance fixtures."
covered_by = [
  "cargo run -p xtask -- conform",
  "cargo run -p xtask -- schema-compat",
]
expires = "2027-01-01"   # optional, only for time-bounded receipts
retired = false           # optional; mark true to keep historical entries
```

### Required fields

| Field | Required | Notes |
|---|---|---|
| `glob` *or* `path` | yes | One of: glob pattern (preferred) or single path. |
| `kind` | yes | Free-form descriptor (e.g. `fixture_input`, `ci_declarative`). |
| `owner` | yes | Crate or team handle. |
| `surface` | yes | One of the surface vocabulary below. |
| `classification` | yes | `production` / `test` / `tooling` / `config` / `generated`. |
| `reason` | yes | One sentence: why a non-Rust file is the right tool here. |
| `covered_by` | conditional | Required for `production` and `test`. List the commands that exercise these files. |
| `generated_by` | conditional | Required for `generated`. Command that regenerates the file. |
| `expires` | optional | ISO date. Defaults to "no expiry" for stable surfaces. |
| `retired` | optional | If `true`, the entry is allowed to match no files (kept for history). |

### Surfaces

| Surface | Examples |
|---|---|
| `ci` | GitHub Actions, GitLab CI, repo agent configs |
| `editor` | VS Code / JetBrains extensions |
| `fixtures` | Test fixtures, golden files, BDD features |
| `contracts` | Vendored hand-written schemas |
| `schema` | Generated JSON schemas |
| `docs` | Markdown, ADRs, diagrams, license texts |
| `config` | Repo-level static config (deny.toml, mutants.toml, ...) |
| `release` | action.yml and other release-time declaratives |
| `licenses` | LICENSE-* files |
| `badge` | Generated metadata for status badges |

### Classifications

| Class | Meaning |
|---|---|
| `production` | Runs in shipped binaries or affects on-the-wire schema. |
| `test` | Exercised only by `cargo test` (or BDD harness). |
| `tooling` | Development-time only — xtask, editor extensions. |
| `config` | Declarative; not executed. |
| `generated` | Produced by xtask; checked in but not edited by hand. |

## Identity

```text
identity = glob (or path)
```

Entries are matched by their pattern. The checker never matches by file
contents. Globs follow standard `glob` crate semantics (`**` for recursion,
`*` for single segment).

## Failure modes

`cargo run -p xtask -- check-file-policy` fails on:

* **Unallowlisted file** — a non-Rust tracked file matches no entry.
* **Stale receipt** — an entry's `glob`/`path` matches no files in the tree
  and `retired` is not set.
* **Expired entry** — `expires` is in the past.
* **Missing required field** — entry is malformed for its classification
  (e.g., a `production` entry without `covered_by`).

## Generated files

Generated artifacts must declare `generated_by`. The checker verifies the
artifact exists, the regeneration command is in `policy/non-rust-allowlist.toml`,
and (where applicable) a paired `*-check` command exists.

Example:

```toml
[[allow]]
glob = "schemas/**"
kind = "schema"
owner = "perfgate-core"
surface = "schema"
classification = "generated"
reason = "schemars-generated JSON Schemas, byte-locked against the source types."
generated_by = "cargo run -p xtask -- schema"
covered_by = ["cargo run -p xtask -- schema-check"]
```

## Repo-class

perfgate is **pure Rust + CLI + service** with one editor extension surface
(`vscode-perfgate/`). The non-Rust footprint is therefore:

* CI declaratives (GitHub Actions, GitLab CI templates)
* Fixtures and contracts
* Documentation (Markdown + diagrams)
* The VS Code extension (TypeScript + manifests)
* Schemas (vendored + generated)

There are no Python, Bash, or Ruby production scripts. If one is added, it
must come with an entry, an owner, and a covered_by command — typically a
sibling Rust test that shells out and asserts behavior.
