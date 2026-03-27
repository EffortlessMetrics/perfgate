# perfgate-sha256

Minimal SHA-256 implementation for perfgate fingerprinting.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

A `#![no_std]`-compatible SHA-256 hash function returning a hexadecimal string.
Designed for fingerprinting and identification, not cryptographic security.
Zero external dependencies.

## Features

- `std` (default) — enables `std` support
- Without `std` — uses `alloc::string::String` only

## Key API

- `sha256_hex(data: &[u8])` — compute SHA-256 and return a 64-char lowercase hex string

## Example

```rust
use perfgate_sha256::sha256_hex;

let hash = sha256_hex(b"hello");
assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
assert_eq!(hash.len(), 64);

let empty = sha256_hex(b"");
assert_eq!(empty, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
```

## License

Licensed under either Apache-2.0 or MIT.
