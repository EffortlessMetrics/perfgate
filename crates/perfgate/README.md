# perfgate

Unified facade crate that re-exports the core perfgate ecosystem.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## When to use

If you want **one dependency** instead of picking individual micro-crates,
add `perfgate` and access everything through its module re-exports:

```toml
[dependencies]
perfgate = "0.15"
```

## What is included

Re-exports 15 crates as modules: `types`, `domain`, `adapters`, `app`,
`budget`, `stats`, `significance`, `error`, `validation`, `render`,
`export`, `sensor`, `paired`, `host_detect`, `sha256`.

A `prelude` module re-exports the most common types and use-case structs
(`RunReceipt`, `CompareReceipt`, `RunBenchUseCase`, etc.) for quick starts.

```rust
use perfgate::prelude::*;
use perfgate::domain::compare_runs;
use perfgate::stats::percentile;
```

## License

Licensed under either Apache-2.0 or MIT.
