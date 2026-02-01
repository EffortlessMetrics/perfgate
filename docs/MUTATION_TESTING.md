# Mutation Testing Guide for perfgate

This document describes the mutation testing infrastructure and baseline for the perfgate project.

## Overview

Mutation testing is a technique that introduces small changes (mutants) to source code to verify that tests detect these changes. A "killed" mutant means tests caught the change; a "surviving" mutant indicates a gap in test coverage.

perfgate uses [cargo-mutants](https://mutants.rs/) for mutation testing.

## Installation

```bash
cargo install cargo-mutants
```

## Running Mutation Tests

### Via xtask (Recommended)

```bash
# Run mutation testing on all configured crates
cargo run -p xtask -- mutants

# Run mutation testing on a specific crate
cargo run -p xtask -- mutants --crate perfgate-domain

# Run with summary report
cargo run -p xtask -- mutants --crate perfgate-domain --summary

# Pass additional arguments to cargo-mutants
cargo run -p xtask -- mutants -- --jobs 4
```

### Direct cargo-mutants

```bash
# Run on all crates
cargo mutants

# Run on specific crate
cargo mutants --package perfgate-domain
```

## Configuration

The mutation testing configuration is defined in `mutants.toml` at the project root:

```toml
# Exclude test code and generated code
exclude_globs = [
    "**/tests/**",
    "**/proptest-regressions/**",
    "**/benches/**",
    "fuzz/**",
    "xtask/**",
]

# Focus on high-value targets
include_globs = [
    "crates/perfgate-domain/src/**",
    "crates/perfgate-types/src/**",
    "crates/perfgate-app/src/**",
]

# Timeout per mutant (60s to allow for property-based tests)
timeout = 60
minimum_test_timeout = 20
```

## Target Kill Rates by Crate

| Crate | Target Kill Rate | Rationale |
|-------|------------------|-----------|
| perfgate-domain | 100% | Pure logic, fully testable |
| perfgate-types | 95% | Serialization logic, some derive macros |
| perfgate-app | 90% | Rendering logic, some formatting edge cases |
| perfgate-adapters | 80% | Platform-specific code, harder to test |
| perfgate-cli | 70% | I/O heavy, integration tested instead |

## Initial Baseline (perfgate-domain)

### Status: Infrastructure Ready

The mutation testing infrastructure is fully configured and ready for use. The actual mutation testing run should be performed:

1. **Manually** during development to identify test gaps
2. **In CI** on a scheduled basis (weekly recommended)

### Running the Baseline

To establish the initial mutation testing baseline for perfgate-domain:

```bash
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

**Note:** This command may take 10-30 minutes depending on hardware, as it:
1. Generates all possible mutants for the crate
2. Runs the full test suite against each mutant
3. Reports which mutants were killed vs survived

### Expected Output

After running, results are stored in `mutants.out/`:

- `outcomes.json` - Machine-readable results
- `caught.txt` - List of killed mutants (tests detected the change)
- `missed.txt` - List of surviving mutants (tests need improvement)
- `timeout.txt` - Mutants that caused test timeout

### Interpreting Results

The summary report shows:
- **Total mutants**: Number of mutations generated
- **Killed**: Mutants detected by tests ✅
- **Survived**: Mutants NOT detected by tests ⚠️
- **Timeout**: Mutants that caused tests to hang
- **Kill rate**: Percentage of killed mutants

### Addressing Surviving Mutants

When mutants survive, review `mutants.out/missed.txt` to identify:

1. **Missing test coverage**: Add tests for the uncovered code path
2. **Weak assertions**: Strengthen assertions to detect the mutation
3. **Equivalent mutants**: Some mutations don't change behavior (rare)

Example surviving mutant entry:
```
replace summarize_u64 -> Option<Summary<u64>> with None in crates/perfgate-domain/src/lib.rs
```

This indicates a test should verify that `summarize_u64` returns `Some(...)` for valid inputs.

## CI Integration

Mutation testing is configured to run on a scheduled basis in CI:

- **Frequency**: Weekly (to avoid slowing down PR builds)
- **Workflow**: `.github/workflows/mutation.yml`
- **Artifacts**: Mutation reports uploaded for review

## Best Practices

1. **Run locally before PRs**: Check mutation coverage for changed code
2. **Focus on domain logic**: Prioritize 100% kill rate for pure functions
3. **Don't chase 100% everywhere**: I/O-heavy code is harder to mutation test
4. **Review surviving mutants**: Each survivor indicates a potential test gap
5. **Use property-based tests**: They often kill mutants that example tests miss

## Troubleshooting

### Mutation testing is slow

- Use `--jobs N` to parallelize (default: number of CPUs)
- Focus on specific crates with `--crate`
- Increase timeout if tests are timing out

### Many timeouts

- Check if property-based tests are running too many iterations
- Increase `timeout` in `mutants.toml`
- Consider excluding slow test files

### False positives (equivalent mutants)

Some mutations don't change observable behavior. These are rare but can be:
- Ignored if clearly equivalent
- Documented in code comments
- Excluded via `#[mutants::skip]` attribute (use sparingly)
