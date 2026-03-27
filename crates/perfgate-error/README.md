# perfgate-error

Unified error types for the perfgate ecosystem.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

This crate provides a single, comprehensive error type (`PerfgateError`) that
unifies all error variants across the perfgate crates, enabling seamless error
propagation and conversion.

## Error Categories

| Category     | Type                    | Description                          |
|--------------|-------------------------|--------------------------------------|
| Validation   | `ValidationError`       | Bench name and input validation      |
| Stats        | `StatsError`            | Statistical computation errors       |
| Adapter      | `AdapterError`          | Process execution, timeout, platform |
| Config       | `ConfigValidationError` | Configuration parsing/validation     |
| IO           | `IoError`               | File system and network I/O          |
| Paired       | `PairedError`           | Paired benchmark errors              |

## Key API

- `PerfgateError` — unified error enum with `From` impls for all sub-errors
- `ValidationError` — bench name validation errors
- `validate_bench_name(name)` — validate a bench name against naming rules
- `ErrorCategory` — categorization enum for error routing
- `Result<T>` — type alias for `std::result::Result<T, PerfgateError>`

## Example

```rust
use perfgate_error::{PerfgateError, ValidationError, validate_bench_name};

fn check(name: &str) -> Result<(), PerfgateError> {
    validate_bench_name(name)?;
    Ok(())
}

let err = check("").unwrap_err();
assert!(matches!(err, PerfgateError::Validation(ValidationError::Empty)));
assert_eq!(err.exit_code(), 1);
```

## License

Licensed under either Apache-2.0 or MIT.
