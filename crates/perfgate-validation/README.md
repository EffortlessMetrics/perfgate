# perfgate-validation

Schema validation and contract testing for benchmark names in perfgate.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Problem

Benchmark names appear in file paths, JSON keys, and REST URLs. A bad name
(path traversal, illegal characters, excessive length) can break storage,
confuse queries, or open security holes. This crate provides a single
validation entry point that enforces strict naming rules everywhere.

## Naming Rules

1. Must not be empty.
2. Maximum 64 characters (`BENCH_NAME_MAX_LEN`).
3. Lowercase ASCII letters, digits, `_`, `.`, `-`, `/` only (`BENCH_NAME_PATTERN`).
4. No empty path segments -- leading, trailing, or consecutive `/` forbidden.
5. No `.` or `..` segments -- path traversal forbidden.

## Key API

- `validate_bench_name(name)` -- returns `Ok(())` or a `ValidationError`
- `ValidationError` -- enum: `Empty`, `TooLong`, `InvalidCharacters`, `EmptySegment`, `PathTraversal`
- `BENCH_NAME_MAX_LEN` -- `64`
- `BENCH_NAME_PATTERN` -- `^[a-z0-9_.\-/]+$`

All types are re-exported from `perfgate-error` for a focused, dependency-light
entry point.

## Example

```rust
use perfgate_validation::{validate_bench_name, ValidationError};

assert!(validate_bench_name("ci/build-time").is_ok());
assert!(validate_bench_name("path/to/bench.v2").is_ok());

assert!(matches!(validate_bench_name(""), Err(ValidationError::Empty)));
assert!(matches!(validate_bench_name("MyBench"), Err(ValidationError::InvalidCharacters { .. })));
assert!(matches!(validate_bench_name("../escape"), Err(ValidationError::PathTraversal { .. })));
```

## License

Licensed under either Apache-2.0 or MIT.
