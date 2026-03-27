# Pre-Merge Review Checklist

This checklist captures recurring bugs and pitfalls found during PR reviews.
Every item below has been a real defect in this codebase or its CI integrations.
Use it as a gate before approving any PR.

---

## Platform & CI

### 1. SQLite in-memory databases silently reject WAL mode

`PRAGMA journal_mode=WAL` on a `:memory:` database returns `"memory"`, not
`"wal"`. Code that assumes WAL is active will silently run without it.

**Wrong:**
```rust
conn.execute_batch("PRAGMA journal_mode=WAL;")?;
// Assumes WAL is now active — no verification
```

**Right:**
```rust
let mode: String = conn.query_row(
    "PRAGMA journal_mode=WAL;",
    [],
    |row| row.get(0),
)?;
if mode != "wal" {
    // Expected for :memory: databases — skip WAL
    tracing::debug!("WAL not available (journal_mode={mode}), continuing without it");
}
```

**Why it matters:** WAL mode enables concurrent readers. Silently falling back
means performance degrades under load with no visible error.

---

### 2. `execute_batch` discards PRAGMA return values

SQLite PRAGMAs that return results (e.g., `journal_mode`, `foreign_keys`) have
their return values silently discarded by `execute_batch`. You cannot detect
failures.

**Wrong:**
```rust
conn.execute_batch(
    "PRAGMA journal_mode=WAL;
     PRAGMA foreign_keys=ON;"
)?;
```

**Right:**
```rust
let mode: String = conn.query_row("PRAGMA journal_mode=WAL;", [], |r| r.get(0))?;
assert_eq!(mode, "wal", "WAL mode not activated");

let fk: bool = conn.query_row("PRAGMA foreign_keys;", [], |r| r.get(0))?;
assert!(fk, "foreign keys not enabled");
```

**Why it matters:** PRAGMAs can fail silently (e.g., WAL on in-memory DBs,
foreign keys on older builds). Without checking, your safety invariants are not
actually enforced.

---

### 3. Windows PDB lock contention during parallel builds

On Windows, parallel `cargo build` invocations fight over the same `.pdb`
(program database) file, causing `fatal error C1041: cannot open program
database`.

**Wrong:**
```yaml
# CI config running multiple cargo builds in parallel with full parallelism
- cargo build --all
- cargo test --all  # may overlap with build
```

**Right:**
```yaml
# Limit job parallelism on Windows runners
- cargo build --all -j4
- cargo test --all -j4
```

**Why it matters:** CI flakes on Windows that look random are often PDB lock
contention. Limiting parallelism (`-j4`) or staggering build/test steps
eliminates the issue.

---

### 4. Bitbucket has no built-in `cargo` cache

Unlike `node` and `pip`, Bitbucket Pipelines does not recognize `cargo` as a
built-in cache type. Referencing `caches: - cargo` without a `definitions` block
causes a pipeline error.

**Wrong:**
```yaml
pipelines:
  default:
    - step:
        caches:
          - cargo  # Error: unknown cache type
```

**Right:**
```yaml
definitions:
  caches:
    cargo: ~/.cargo

pipelines:
  default:
    - step:
        caches:
          - cargo
```

**Why it matters:** The pipeline fails immediately with a confusing error about
an unknown cache, blocking all builds until the definitions block is added.

---

### 5. CircleCI `environment` block does not interpolate variables

`${VAR}` in CircleCI's `environment` key is treated as a literal string, not
expanded. This is unlike shell commands where interpolation works normally.

**Wrong:**
```yaml
jobs:
  test:
    environment:
      PERFGATE_TOKEN: ${VAULT_TOKEN}  # Literal string "${VAULT_TOKEN}"
```

**Right:**
```yaml
# Set PERFGATE_TOKEN as a project-level environment variable in CircleCI UI,
# or use a shell command to export it:
jobs:
  test:
    steps:
      - run: export PERFGATE_TOKEN="$VAULT_TOKEN" && cargo test
```

**Why it matters:** Authentication silently fails because the token is the
literal string `${VAULT_TOKEN}` instead of the actual secret.

---

### 6. Bitbucket does not upload artifacts from failed steps

If a step exits non-zero, Bitbucket skips artifact collection. When perfgate
exits with code 2 (budget violation), the report artifacts are lost.

**Wrong:**
```yaml
- step:
    script:
      - perfgate check --config perfgate.toml --bench my-bench
    artifacts:
      - artifacts/perfgate/**
```

**Right:**
```yaml
- step:
    script:
      - |
        EXIT=0
        perfgate check --config perfgate.toml --bench my-bench || EXIT=$?
        # Artifacts are collected before we propagate the exit code
        exit $EXIT
    artifacts:
      - artifacts/perfgate/**
```

**Why it matters:** The whole point of perfgate artifacts is to diagnose budget
violations. Losing them on failure defeats the purpose.

---

### 7. CircleCI `store_artifacts` defaults to `on_success`

CircleCI skips `store_artifacts` when a previous step fails. Since perfgate
exits with code 2 on budget violations, artifacts are not stored when you need
them most.

**Wrong:**
```yaml
steps:
  - run: perfgate check ...
  - store_artifacts:
      path: artifacts/perfgate
```

**Right:**
```yaml
steps:
  - run: perfgate check ...
  - store_artifacts:
      path: artifacts/perfgate
      when: always
```

**Why it matters:** Same as the Bitbucket issue above: artifacts are most
valuable when a budget check fails, so they must be collected unconditionally.

---

### 8. Chart.js cannot resolve CSS custom properties

Canvas 2D rendering context does not have access to the DOM's computed styles.
`var(--color-primary)` resolves to the empty string, which Chart.js interprets
as black.

**Wrong:**
```javascript
datasets: [{
    borderColor: 'var(--color-primary)',  // Renders as black
    data: points,
}]
```

**Right:**
```javascript
datasets: [{
    borderColor: '#3b82f6',  // Hardcoded hex color
    data: points,
}]
```

**Why it matters:** All chart lines render as black, making multi-series charts
unreadable. The bug is silent — no console errors.

---

## Precision & Math

### 9. Nanosecond-to-millisecond conversion must preserve sub-millisecond precision

Integer division (`ns / 1_000_000`) truncates fractional milliseconds. For
floating-point statistics fields (mean, stddev, p95), this loses meaningful
precision.

**Wrong:**
```rust
fn ns_to_ms(ns: u64) -> f64 {
    (ns / 1_000_000) as f64  // Integer division first — 1_500_000 becomes 1.0
}
```

**Right:**
```rust
fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0  // Float division — 1_500_000 becomes 1.5
}
```

**Why it matters:** Sub-millisecond regressions are invisible. A 0.5ms stddev
becomes 0.0ms, making budget checks incorrectly pass.

---

### 10. Floor clamping to 1ms masks zero-time measurements

Applying `.max(1)` to timing values forces 0ns measurements to 1ms. This
inflates baselines and hides genuinely fast operations.

**Wrong:**
```rust
let duration_ms = raw_ns.max(1) / 1_000_000;
// A 0ns measurement becomes 1ns, then 0ms after division — inconsistent
// A 500ns measurement becomes 500ns, then 0ms — same problem
```

**Right:**
```rust
let duration_ms = raw_ns as f64 / 1_000_000.0;
// Only clamp if you have a specific reason, and document why:
// let duration_ms = if raw_ns > 0 && raw_ns < 1_000_000 {
//     0.001  // Sub-ms but nonzero
// } else {
//     raw_ns as f64 / 1_000_000.0
// };
```

**Why it matters:** Artificial floors distort statistical comparisons. A
benchmark that genuinely completes in 0ns (e.g., a no-op baseline) should not
report 1ms.

---

## Security

### 11. XSS in dynamic HTML generation

User-controlled values (benchmark names, metric labels, verdict strings) must be
escaped before insertion into HTML. The `perfgate-export` HTML exporter and any
dashboard templates are attack surfaces.

**Wrong:**
```javascript
container.innerHTML = `<h2>${benchmarkName}</h2>`;
// benchmarkName = '<img src=x onerror=alert(1)>' → XSS
```

**Right:**
```javascript
function escHtml(s) {
    return s
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}
container.innerHTML = `<h2>${escHtml(benchmarkName)}</h2>`;
```

In Rust (for the HTML exporter):
```rust
fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#39;")
}
```

**Why it matters:** Benchmark names come from user config files and can contain
arbitrary strings. If rendered unescaped in a browser (CI dashboard, HTML
export), this is a stored XSS vulnerability.

---

## Code Quality

### 12. Unify identical platform implementations

When `#[cfg(unix)]` and `#[cfg(windows)]` blocks contain byte-for-byte
identical code (e.g., `try_wait()` polling loops), they should be unified with
`#[cfg(any(unix, windows))]`.

**Wrong:**
```rust
#[cfg(unix)]
fn poll_child(child: &mut Child) -> io::Result<Option<ExitStatus>> {
    child.try_wait()
}

#[cfg(windows)]
fn poll_child(child: &mut Child) -> io::Result<Option<ExitStatus>> {
    child.try_wait()  // Identical — unnecessary duplication
}
```

**Right:**
```rust
#[cfg(any(unix, windows))]
fn poll_child(child: &mut Child) -> io::Result<Option<ExitStatus>> {
    child.try_wait()
}
```

**Why it matters:** Duplicated platform blocks are a maintenance hazard. A bug
fix in one block is easily missed in the other. Unified code with
`cfg(any(...))` keeps a single source of truth.

---

### 13. Verify Rust struct field names against actual serde output

Rust's `#[serde(rename)]` and `#[serde(rename_all = "snake_case")]` can produce
field names that differ from what you'd guess. Always check the actual
serialized JSON, not your intuition.

**Wrong:**
```javascript
// Guessing the field name from the Rust struct
const id = data.run_id;  // Actual serialized name is "id" (serde rename)
```

**Right:**
```javascript
// Verified against actual JSON output:
// { "id": "abc123", "name": "my-bench", ... }
const id = data.id;
```

**Verification method:**
```bash
# Serialize a struct and inspect the output
cargo test -p perfgate-types -- --nocapture test_serialization 2>&1 | head -20
# Or check the JSON Schema:
cat schemas/perfgate.run.v1.schema.json | jq '.properties | keys'
```

**Why it matters:** A wrong field name produces `undefined` in JavaScript or a
deserialization error in other languages, often silently breaking dashboards or
downstream consumers.

---

## Quick Reference Checklist

Use this as a copy-paste checklist in PR reviews:

```markdown
### Pre-Merge Checklist
- [ ] SQLite PRAGMAs use `query_row` and verify return values (not `execute_batch`)
- [ ] WAL mode is skipped or verified for in-memory databases
- [ ] Nanosecond conversions use float division (`/ 1_000_000.0`), not integer
- [ ] No artificial floor clamping on timing values without documented reason
- [ ] User-controlled strings are HTML-escaped before DOM insertion
- [ ] Chart.js colors use hex values, not CSS custom properties
- [ ] Platform-specific code is unified where implementations are identical
- [ ] JS field names match actual serde-serialized JSON, not Rust struct names
- [ ] CI artifacts use `when: always` (CircleCI) or deferred exit (Bitbucket)
- [ ] CI env vars are not using `${VAR}` interpolation in CircleCI `environment`
- [ ] Windows CI uses `-j4` or staggered builds to avoid PDB lock contention
- [ ] Bitbucket pipelines define custom cache types in `definitions` block
```
