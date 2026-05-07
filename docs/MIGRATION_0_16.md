# Migration Guide: 0.16.0 Public API Refactoring

This guide helps users update their code as perfgate's public crate surface changes in version 0.16.0 and beyond.

---

## What's Changing?

For the 0.16 release line, perfgate's public package surface is being
reorganized from many microcrates to **5 main crates** with strongly organized
internal modules.

The collapse is landing PR by PR. Do not assume every old microcrate receives a
new 0.16 compatibility release. Some internal crates may be deleted directly,
while already-public paths that need a transition can remain as compatibility
wrappers for one release.

---

## New Public Crate Surface

| Crate | Purpose | New in 0.16? |
|-------|---------|------------|
| `perfgate` | Main embeddable library and facade | Primary entry point |
| `perfgate-types` | Stable receipts, config, schemas | For parsing JSON receipts |
| `perfgate-cli` | Installs the `perfgate` binary | No change |
| `perfgate-client` | Baseline service client | No change |
| `perfgate-server` | Baseline service binary/library | No change |

All other perfgate crates are moving toward internal-module status inside the
public packages above.

---

## What Changed and How to Update

### If you import from `perfgate-*` microcrates

**Before (0.15.x)**:
```rust
use perfgate_stats::summarize_u64;
use perfgate_budget::evaluate_budget;
use perfgate_domain::CompareResult;
use perfgate_render::markdown;
```

**After the relevant absorption PR lands**:
```rust
use perfgate_domain::stats::summarize_u64;
use perfgate::core::budget::evaluate_budget;
use perfgate::domain::CompareResult;
use perfgate::presentation::render::markdown;
```

---

### Crate-to-Module Mapping

Use this table to find the new import path for any old crate:

| Old Crate | Current / Target Module | Example |
|-----------|-------------------------|---------|
| `perfgate-stats` | `perfgate_domain::stats` now; facade path later | `use perfgate_domain::stats::summarize_u64;` |
| `perfgate-budget` | `perfgate_domain::budget` now; `perfgate::core::budget` facade path | `use perfgate::core::budget::evaluate_budget;` |
| `perfgate-significance` | `perfgate::core::significance` | `use perfgate::core::significance::*;` |
| `perfgate-paired` | `perfgate_domain::paired` now; facade path later | `use perfgate_domain::paired::*;` |
| `perfgate-sha256` | `perfgate_types::fingerprint` now; `perfgate::core::fingerprint` facade path | `use perfgate::core::fingerprint::*;` |
| `perfgate-domain` | `perfgate::domain` | `use perfgate::domain::*;` |
| `perfgate-host-detect` | `perfgate_domain::host` now; `perfgate::domain::host` facade path | `use perfgate::domain::host::*;` |
| `perfgate-scaling` | `perfgate::domain::scaling` | `use perfgate::domain::scaling::*;` |
| `perfgate-render` | `perfgate::presentation::render` | `use perfgate::presentation::render::*;` |
| `perfgate-export` | `perfgate::presentation::export` | `use perfgate::presentation::export::*;` |
| `perfgate-sensor` | `perfgate::presentation::sensor` | `use perfgate::presentation::sensor::*;` |
| `perfgate-summary` | `perfgate::presentation::summary` | `use perfgate::presentation::summary::*;` |
| `perfgate-error` | `perfgate_types::error` | `use perfgate_types::error::*;` |
| `perfgate-validation` | `perfgate_types::validation` | `use perfgate_types::validation::*;` |
| `perfgate-auth` | `perfgate_api::auth` now; final owner TBD | `use perfgate_api::auth::*;` |
| `perfgate-config` | `perfgate_types::config` | `use perfgate_types::config::*;` |
| `perfgate-api` | `perfgate_types::baseline_service` | `use perfgate_types::baseline_service::*;` |
| `perfgate-fake` | private workspace crate | No public replacement yet; keep local test helpers in your own crate. |
| `perfgate-adapters` | `perfgate::runtime` | `use perfgate::runtime::*;` |
| `perfgate-app` | `perfgate::app` | `use perfgate::app::*;` |

---

### In Your Cargo.toml

**Before (0.15.x)**:
```toml
[dependencies]
perfgate = "0.15"
perfgate-stats = "0.15"
perfgate-budget = "0.15"
perfgate-domain = "0.15"
perfgate-types = "0.15"
```

**Target after the 0.16 collapse**:
```toml
[dependencies]
perfgate = "0.16"
perfgate-types = "0.16"  # Only if you need types directly

# No need to depend on perfgate-stats, perfgate-budget, perfgate-domain, etc.
# They are now internal modules accessed through the main crate.
```

---

### Optional Features

After the feature-gated facade modules land, some modules are behind feature flags:

```toml
[dependencies]
perfgate = { version = "0.16", features = ["render", "export", "github"] }
perfgate-types = "0.16"
```

Available features:
- `core` - Core stats, budget, significance (enabled by default)
- `domain` - Domain logic and policies (enabled by default)
- `runtime` - System adapters and I/O (enabled by default)
- `render` - Markdown and terminal rendering
- `export` - CSV, JSONL, HTML, Prometheus export
- `sensor` - Cockpit mode and sensor reports
- `github` - GitHub annotations and integration
- `ingest` - Data ingestion adapters

---

## Examples

### Example 1: Computing Statistics

**Before (0.15.x)**:
```rust
use perfgate_stats::summarize_u64;

fn analyze() -> Result<(), Box<dyn std::error::Error>> {
    let stats = summarize_u64(&[10, 30, 20])?;
    println!("Median: {}", stats.median);
    Ok(())
}
```

**After the stats absorption**:
```rust
use perfgate_domain::stats::summarize_u64;

fn analyze() -> Result<(), Box<dyn std::error::Error>> {
    let stats = summarize_u64(&[10, 30, 20])?;
    println!("Median: {}", stats.median);
    Ok(())
}
```

### Example 2: Evaluating Budgets

**Before (0.15.x)**:
```rust
use perfgate_budget::evaluate_budget;
use perfgate_types::{Budget, MetricStatus};

fn check(budget: &Budget) {
    let result = evaluate_budget(100.0, 115.0, budget, None).unwrap();
    assert_eq!(result.status, MetricStatus::Warn);
}
```

**Target after the corresponding absorption PR**:
```rust
use perfgate::core::budget::evaluate_budget;
use perfgate_types::{Budget, MetricStatus};

fn check(budget: &Budget) {
    let result = evaluate_budget(100.0, 115.0, budget, None).unwrap();
    assert_eq!(result.status, MetricStatus::Warn);
}
```

### Example 3: Comparing Runs

**Before (0.15.x)**:
```rust
use perfgate_domain::compare;
use perfgate_types::{Run, CompareConfig};

fn compare_runs(baseline: &Run, current: &Run, config: &CompareConfig) {
    let result = compare(baseline, current, config);
    println!("Comparison: {:?}", result);
}
```

**Target after the corresponding absorption PR**:
```rust
use perfgate::domain::compare;
use perfgate_types::{Run, CompareConfig};

fn compare_runs(baseline: &Run, current: &Run, config: &CompareConfig) {
    let result = compare(baseline, current, config);
    println!("Comparison: {:?}", result);
}
```

### Example 4: Using Test Fixtures

**Before (0.15.x)**:
```rust
use perfgate_fake::fake_run;

#[test]
fn test_my_thing() {
    let run = fake_run();
    assert!(!run.samples.is_empty());
}
```

**After the public-surface collapse**:

`perfgate-fake` is a workspace-private crate. There is no supported public
replacement in 0.16 yet. If you depended on it, copy the small fixture helpers
you need into your own test module or build receipts directly with
`perfgate-types`.

For example:

```rust
#[cfg(test)]
mod test_support;

#[test]
fn test_my_thing() {
    let run = test_support::fake_run();
    assert!(!run.samples.is_empty());
}
```

---

## What About Deprecation Warnings?

During the transition, some old crate paths may remain as compatibility shims
and emit deprecation warnings:

```
warning: use of deprecated crate `perfgate_stats`
  --> src/main.rs:1:5
   |
1  | use perfgate_stats::describe;
   |     ^^^^^^^^^^^^^^
   |
   = note: Use perfgate::core::stats instead
```

**Action**: Update your imports to the new paths. The warnings will go away.

**Deadline**: Compatibility shims are temporary. Internal-only crates may be
deleted without a new shim release, so prefer the owner paths listed in
`policy/absorbed_crates.txt`.

---

## Troubleshooting

### "Module not found" after updating to 0.16.0

Make sure you have:
1. Updated `Cargo.toml` to use `perfgate = "0.16"`
2. Updated your imports to use the new module paths from the mapping table
3. Added necessary features if you use render, export, or other optional modules

### Old crate import still works but shows deprecation warning

That crate is currently a compatibility shim. Update your imports to the owner
path listed in this guide or `policy/absorbed_crates.txt`.

### Feature not available in the module I'm importing

Check if the module is feature-gated. For example, if you import from `perfgate::presentation::render`, add the `render` feature to Cargo.toml:

```toml
[dependencies]
perfgate = { version = "0.16", features = ["render"] }
```

### Can't find a symbol that was in the old crate

The symbol might have moved to a different module or been renamed. Check the old crate's documentation and cross-reference with the new module path.

---

## Frequently Asked Questions

### Will my code break when I update to 0.16.0?

**No**. In 0.16.0, old imports still work with deprecation warnings. You have until 0.17.0 (or later) to update your code.

### Do I need to update my Cargo.toml?

Not immediately in 0.16.0. But to remove deprecation warnings and prepare for 0.17.0, update your `Cargo.toml`:
1. Remove `perfgate-stats`, `perfgate-budget`, etc. from dependencies
2. Keep only `perfgate` and `perfgate-types`
3. Add features if you use optional modules

### What if I'm using many internal crates?

Use the mapping table above to find the new module path for each old crate. Then update imports in your code.

### Should I use `perfgate` or the individual internal modules?

Use `perfgate::prelude::*` for most common types and functions:

```rust
use perfgate::prelude::*;
```

This gives you clean access to the public API without importing every module. For specific modules (like `perfgate::core::stats`), import them directly as needed.

---

## Timeline

| Version | Status | Action |
|---------|--------|--------|
| 0.15.x and earlier | Published | Broad microcrate surface |
| 0.16.0 | Target release line | Owner modules plus temporary shims where needed |
| 0.17.0+ | Future | Remove remaining transition shims after migration |

---

## Resources

- [REFACTORING_0_16.md](REFACTORING_0_16.md) - The refactoring strategy
- [CRATE_SEAMS.md](CRATE_SEAMS.md) - Why the refactoring happened
- [docs/ARCHITECTURE.md](ARCHITECTURE.md) - Updated architecture documentation
- [CHANGELOG.md](../CHANGELOG.md) - Release notes for 0.16.0

---

## Questions?

If you encounter issues with the migration, please:
1. Check this guide and the mapping table
2. Consult the updated ARCHITECTURE.md
3. Open an issue on GitHub with your migration question

We want the transition to be smooth. Feedback is welcome!
