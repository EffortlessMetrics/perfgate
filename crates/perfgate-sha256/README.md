# perfgate-sha256

Minimal, dependency-free SHA-256 for deterministic fingerprints.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Why not use a crate?

`perfgate-sha256` sits in the innermost layer of the architecture, where
zero external dependencies is a hard design constraint. A single public
function, `#![no_std]`-compatible, is all that is needed for host and
receipt fingerprinting.

## Key API

- `sha256_hex(data: &[u8]) -> String` -- SHA-256 as a 64-char lowercase hex string

## Features

| Feature | Default | Effect |
|---------|---------|--------|
| `std`   | yes     | Enables `std`; without it, only `alloc` is required |

## Example

```rust
use perfgate_sha256::sha256_hex;

let hash = sha256_hex(b"hello");
assert_eq!(hash.len(), 64);
assert_eq!(
    hash,
    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
);
```

## License

Licensed under either Apache-2.0 or MIT.
