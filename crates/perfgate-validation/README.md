# perfgate-validation

Validation functions for benchmark names and configuration.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

This crate re-exports bench-name validation logic from `perfgate-error` and
provides a focused entry point for validating benchmark names against strict
naming rules.

## Naming Rules

- Lowercase alphanumeric, dots, underscores, hyphens, slashes only
- Maximum 64 characters
- No empty path segments (leading/trailing/consecutive slashes)
- No path traversal (`.` or `..` segments)

## Key API

- `validate_bench_name(name)` — validate a bench name, returns `Result<(), ValidationError>`
- `ValidationError` — error enum (Empty, TooLong, InvalidCharacters, EmptySegment, PathTraversal)
- `BENCH_NAME_MAX_LEN` — maximum allowed length (64)
- `BENCH_NAME_PATTERN` — regex pattern for valid names

## Example

```rust
use perfgate_validation::{validate_bench_name, ValidationError};

assert!(validate_bench_name("my-bench").is_ok());
assert!(validate_bench_name("path/to/bench.v2").is_ok());

assert!(matches!(validate_bench_name(""), Err(ValidationError::Empty)));
assert!(validate_bench_name("MyBench").is_err());   // uppercase
assert!(validate_bench_name("../bench").is_err());   // path traversal
```

## License

Licensed under either Apache-2.0 or MIT.
