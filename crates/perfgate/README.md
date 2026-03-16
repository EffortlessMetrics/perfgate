# perfgate

High-performance, modular Rust library for performance budgeting and baseline diffing.

This is the facade crate for the `perfgate` ecosystem. It re-exports functionality from the core micro-crates to provide a single, unified entry point.

## Architecture

`perfgate` is built on a layered micro-crate architecture:

- **`perfgate::types`**: Versioned receipt and configuration data structures.
- **`perfgate::domain`**: Pure logic for statistics computation and budget comparison.
- **`perfgate::adapters`**: System metrics collection and process execution.
- **`perfgate::app`**: High-level use cases and report rendering.

## Usage

Add `perfgate` to your `Cargo.toml`:

```toml
[dependencies]
perfgate = "0.4.1"
```

### Example: Running a benchmark

```rust
use perfgate::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logic coming soon in higher-level facade helpers
    Ok(())
}
```

## License

Licensed under either Apache-2.0 or MIT.
