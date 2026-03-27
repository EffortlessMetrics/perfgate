# Contributing

## Local Workflow

```bash
# Full CI check: fmt, clippy, test, schema validation, fixture conformance
cargo run -p xtask -- ci
```

## Architecture

perfgate is a 26-crate Rust workspace following clean architecture. The domain
core is I/O-free; platform-specific code lives in adapters.

```
types / error          (innermost — shared contracts)
  ↓
domain / stats / significance / budget / sha256 / host-detect / validation
  ↓
adapters / paired / auth / api / config
  ↓
app / summary / render / export / sensor
  ↓
server / client / cli  (outermost — I/O and user-facing)
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and the
[ADRs](docs/adrs/) for design rationale.

## Repo Automation (xtask)

| Command | Description |
|---------|-------------|
| `ci` | fmt + clippy + test + schema + conform |
| `schema` / `schema-check` | Generate and verify JSON schemas |
| `conform` | Validate fixtures against vendored schemas |
| `sync-fixtures` | Sync golden fixtures to contracts |
| `dogfood fixtures` | Regenerate dogfooding fixtures |
| `dogfood verify` | Validate artifact layout in CI |
| `docs-sync` / `docs-check` | Manage system documentation |
| `mutants` | Run mutation testing |
| `microcrates` | Inventory workspace crates and kill rate targets |

## Changelog

Update [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]` for every PR, following
[Keep a Changelog](https://keepachangelog.com/).

## Schemas

```bash
cargo run -p xtask -- schema
```

Schemas are written to `schemas/`. The vendored `sensor.report.v1` schema at
`contracts/schemas/` is hand-written and not auto-generated.

## Dogfooding

perfgate gates its own performance across three CI lanes (Smoke, Perf, Nightly).
If your changes affect CLI execution or artifact formats, update fixtures:

```bash
cargo run -p xtask -- dogfood fixtures
```

See [docs/SELF_DOGFOODING.md](docs/SELF_DOGFOODING.md) and
[docs/BASELINE_POLICY.md](docs/BASELINE_POLICY.md).

## Testing

See [TESTING.md](TESTING.md) for the full testing guide. Quick reference:

```bash
cargo test --all                                    # all tests
cargo test --test cucumber                          # BDD tests
cargo test -p perfgate-domain                       # single crate
cargo run -p xtask -- mutants --crate perfgate-domain  # mutation testing
```

Mutation testing kill rate targets:
- Core Domain (domain, types, budget, stats): **95-100%**
- Application (app, client, server): **90%**
- Adapters & Infrastructure: **80-85%**
- Presentation (export, render, sensor): **80%**
- CLI: **70%**

See [docs/MUTATION_TESTING.md](docs/MUTATION_TESTING.md) for details.

## Pre-Merge Review Checklist

Before approving any PR, run through the
[Review Checklist](docs/REVIEW_CHECKLIST.md). It covers recurring bugs found
during PR reviews, organized by category:

- **Platform & CI** — SQLite WAL on in-memory DBs, Windows PDB locks, Bitbucket
  artifact collection, CircleCI env var interpolation
- **Precision & Math** — nanosecond conversion truncation, floor clamping
- **Security** — XSS in HTML exports and dashboards
- **Code Quality** — platform code duplication, serde field name mismatches

Each item includes the wrong vs. right pattern with code examples.

### AI-Generated Code

AI-generated PRs require a **separate AI review pass** before merge
([ADR 0014](docs/adrs/0014-ai-review-always-required.md)). During a session that
produced 40+ PRs, every single reviewed PR had real bugs found by a review agent ---
even though the same model wrote the code. See
[Unconventional Findings](docs/UNCONVENTIONAL_FINDINGS.md) for the full analysis of
the review paradox, agent specialization, and merge ordering strategies.

## Fuzzing

Requires nightly. See `fuzz/README.md`.

```bash
cd fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run parse_run_receipt
```
