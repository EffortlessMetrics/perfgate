# Review Bug Catalog

A catalog of every bug found by review agents during the parallel agent
development session. This document serves as a reference for the kinds of bugs
AI implementation agents introduce and how review agents catch them.

---

## Bug Pattern Categories

Across 12 PRs reviewed, review agents identified **21 distinct bugs** falling
into six categories:

| Category | Count | Description |
|----------|------:|-------------|
| Platform knowledge gaps | 7 | Incorrect assumptions about CI platform behavior, OS APIs, or runtime environments |
| Precision / type errors | 6 | Wrong field names, wrong types, integer truncation, derive mismatches |
| API misuse / silent failures | 3 | Calling an API in a way that silently discards errors or returns wrong results |
| Error handling gaps | 2 | Missing fallback paths or incomplete error classification |
| Code quality | 2 | Unnecessary duplication, missing files declared in metadata |
| Security | 1 | XSS via unescaped user input in innerHTML |

---

## Detailed Findings by PR

### PR #86 -- Windows Timeout

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Unix and Windows `wait_timeout` implementations were byte-for-byte identical | Code duplication | Merged into single `#[cfg(any(unix, windows))]` block, removing 18 lines |

**Root cause:** The implementation agent copied the Unix approach for Windows
without noticing the two blocks could be unified into a single
platform-conditional compilation gate.

---

### PR #87 -- CI Platform Guides

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Bitbucket examples referenced `caches: - cargo` (not a built-in cache) | Platform knowledge gap | Removed from non-caching examples; added `definitions` block where caching was needed |
| 2 | CircleCI `environment` block used `${VAR}` syntax (literal string, not interpolated at runtime) | Platform knowledge gap | Removed `environment` block; documented that project-level vars are auto-available |
| 3 | Bitbucket does not upload artifacts from failed steps | Platform knowledge gap | Added `\|\| EXIT=$?` deferred-exit pattern so artifacts upload before the step fails |
| 4 | CircleCI `store_artifacts` defaults to `on_success` only | Platform knowledge gap | Added `when: always` |
| 5 | Bitbucket auto-injects repo variables, making explicit `export` lines redundant | Platform knowledge gap | Removed redundant exports |

**Root cause:** The implementation agent generated CI configuration from general
knowledge without verifying each platform's specific semantics for caching,
variable injection, and artifact collection on failure.

---

### PR #88 -- SQLite WAL Mode

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | `execute_batch` discards PRAGMA return values, so WAL activation failure is silent | API misuse (silent failure) | Switched to `query_row` and verified the returned mode string matches `"wal"` |
| 2 | In-memory SQLite databases cannot use WAL mode | Edge case / platform limitation | Added `is_memory` parameter; skip WAL for `:memory:` connections |

**Root cause:** The agent treated `PRAGMA journal_mode=WAL` as a fire-and-forget
statement. SQLite PRAGMAs that change state return a result indicating whether
the change took effect, and `execute_batch` silently drops that result.

---

### PR #92 -- Dashboard Enhancement

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Chart.js datasets used `var(--color-primary)` -- canvas cannot resolve CSS custom properties | DOM/Canvas API knowledge gap | Hardcoded hex color values |
| 2 | `run.run_id` referenced a non-existent field (actual JSON key: `run.id`) | Serialization name mismatch | Corrected to `run.id` |
| 3 | `run.repeat` was on the wrong struct (belongs to `bench`, not `run`) | Type confusion across API boundary | Changed to `bench.repeat` |
| 4 | `bench.command` is `Vec<String>` in the schema, not `String` | Type confusion | Added `.join(' ')` |
| 5 | `verdictBadge()` passed unescaped status text into `innerHTML` | Security vulnerability (XSS) | Added `escHtml()` sanitization |

**Root cause:** The agent built a JavaScript dashboard against an assumed API
shape rather than the actual serialized JSON schema. The XSS bug is a classic
case of trusting data that flows through innerHTML without escaping.

---

### PR #95 -- Pre-built Binaries

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Archive extraction failure would abort the workflow without fallback to cargo install | Error handling gap | Restructured logic to use an `ok` flag for graceful fallthrough |
| 2 | Binary copy glob could match the archive file itself | Filename collision | Replaced glob with explicit per-platform binary paths |

**Root cause:** The agent wrote the happy path but did not consider what happens
when `tar`/`unzip` fails or when a glob pattern is ambiguous.

---

### PR #96 -- PostgreSQL Pool Hardening

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | `is_transient()` only matched SQLSTATE `57P01` (admin shutdown), missing `57P02` (crash shutdown) and `57P03` (cannot connect now) | Incomplete error classification | Widened match to `57P` prefix |

**Root cause:** The agent hard-coded a single SQLSTATE rather than recognizing
that the entire `57P` class represents transient server-lifecycle conditions.

---

### PR #101 -- Benchmark Ingest

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Criterion mean used integer `ns_to_ms()`, losing sub-millisecond precision | Precision loss (integer truncation) | Used `ns / 1_000_000.0` with f64 arithmetic |
| 2 | Criterion stddev was floor-clamped to 1 ms for sub-ms values | Precision loss (floor clamping) | Same f64 division without floor clamping |
| 3 | Hyperfine `seconds_to_ms` clamped `0.0` to 1 ms | Edge case (zero-time measurement) | Only clamp values strictly between 0 and 1 ms, leave 0.0 as-is |

**Root cause:** The agent applied integer conversion helpers designed for
wall-clock durations to statistical fields (mean, stddev) where sub-millisecond
precision matters.

---

### PR #103 -- Scaling Validation

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | Missing `Arbitrary` derive on `ScalingConfig` | Feature flag compatibility | Added conditional `#[cfg_attr(feature = "arbitrary", derive(Arbitrary))]` |
| 2 | Missing `README.md` declared in `Cargo.toml` but never created | Missing file | Created the README |

**Root cause:** The agent added a new crate and type without checking existing
workspace conventions for feature-flag derives and Cargo.toml metadata
requirements.

---

### PR #104 -- GitHub PR Comment Bot

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | `action.yml` cockpit mode passes `sensor.report.v1` envelope as if it were `perfgate.report.v1` | Schema type confusion | Check for cockpit extras directory first; use inner `perfgate.report.v1.json` instead of the outer envelope `report.json` |

**Root cause:** Cockpit mode wraps the standard report inside a sensor report
envelope. The agent treated the envelope as the inner report, passing the wrong
schema type to downstream commands.

---

### PR #106 -- Fleet Dependency Detection

| # | Finding | Type | Fix |
|---|---------|------|-----|
| 1 | `Default` derive produced `min_affected=0` and `limit=0` instead of the serde defaults of 2 and 50 | Derive vs serde default mismatch | Wrote a manual `Default` impl matching the serde `#[serde(default = "...")]` values |

**Root cause:** The agent added `#[derive(Default)]` without noticing that serde
default functions specified non-zero values. Rust's `Default` for integers is 0,
which silently diverges from the intended configuration defaults.

---

## Observations

### Most common root cause: assumed knowledge

The single most frequent failure mode is the agent generating code based on
what it "thinks" an API, platform, or type looks like rather than verifying
against the actual source of truth. This accounts for all platform knowledge
gaps, all type/field name mismatches, and the schema type confusion.

### Silent failures are the most dangerous

Bugs in PRs #88 and #96 would have passed all tests and only manifested under
specific runtime conditions (in-memory databases, PostgreSQL restarts). These
are the hardest for automated test suites to catch and the most valuable for
review agents to flag.

### Precision bugs cluster in numeric code

Three of the six precision/type bugs were in the same PR (#101) and all
involved the same conceptual error: applying integer-granularity helpers to
floating-point statistical data. Review agents that check unit consistency
across conversion boundaries are particularly effective here.

### Security bugs are rare but high-impact

Only one XSS bug was found across the entire session, but it was in
user-facing HTML rendering code. A single review-agent rule ("flag any
unescaped interpolation into innerHTML") would catch this class entirely.
