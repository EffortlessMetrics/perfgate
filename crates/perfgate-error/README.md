# perfgate-error

Shared error types and categorization across the perfgate workspace.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Problem

A workspace with 26 crates needs a single error vocabulary so that errors flow
cleanly from inner layers (stats, adapters) through orchestration to the CLI
exit code. This crate defines that vocabulary.

## Error Categories

| Category | Type | Typical Cause |
|----------|------|---------------|
| Validation | `ValidationError` | Invalid bench name, bad characters, path traversal |
| Stats | `StatsError` | No samples to summarize |
| Adapter | `AdapterError` | Spawn failure, timeout, unsupported platform |
| Config | `ConfigValidationError` | Bad config file, invalid bench reference |
| IO | `IoError` | Missing baseline file, artifact write failure |
| Paired | `PairedError` | Paired benchmark with no samples |
| Auth | `AuthError` | Missing/expired key, insufficient permissions |

All variants unify under `PerfgateError` with `From` impls for seamless `?`
propagation. Every error maps to exit code **1** (vs policy-fail `2` / warn `3`).

## Key API

- `PerfgateError` -- unified enum wrapping all category-specific errors
- `ErrorCategory` -- classification enum for routing and diagnostics
- `is_recoverable()` -- true for transient failures (I/O, timeouts)
- `exit_code()` -- always `1` for tool/runtime errors
- `validate_bench_name(name)` -- bench name validation
- `Result<T>` -- type alias for `std::result::Result<T, PerfgateError>`

## Example

```rust
use perfgate_error::{PerfgateError, ValidationError, validate_bench_name};

let err = validate_bench_name("../escape").unwrap_err();
let unified: PerfgateError = err.into();
assert_eq!(unified.exit_code(), 1);
assert!(!unified.is_recoverable());
```

## License

Licensed under either Apache-2.0 or MIT.
