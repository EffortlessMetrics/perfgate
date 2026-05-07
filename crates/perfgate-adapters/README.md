# perfgate-adapters

Workspace-only compatibility wrapper for runtime process and host adapters.

The implementation now lives in `perfgate::runtime` and is exposed through
the public facade at `perfgate::runtime`. This package is marked
`publish = false` during the 0.16 public-surface collapse.

## Migration

Prefer the facade path in new code:

```rust
use perfgate::runtime::{CommandSpec, ProcessRunner, StdProcessRunner};
```

Workspace-internal crates should use the owner module directly:

```rust
use perfgate::runtime::{CommandSpec, ProcessRunner};
```

## License

Licensed under either Apache-2.0 or MIT.
