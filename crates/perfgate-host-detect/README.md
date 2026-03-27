# perfgate-host-detect

Host mismatch detection for benchmarking noise reduction.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

Detects host environment differences between baseline and current benchmark
runs. Host mismatches can introduce significant noise into performance
measurements, leading to false positives or negatives.

## Detection Criteria

| Signal         | Threshold             | Example                       |
|----------------|-----------------------|-------------------------------|
| OS             | Any difference        | `linux` vs `windows`          |
| Architecture   | Any difference        | `x86_64` vs `aarch64`        |
| CPU count      | > 2× ratio            | 4 vs 16 CPUs                 |
| Memory         | > 2× ratio            | 8 GB vs 32 GB                |
| Hostname hash  | Different (if both set) | Different machines          |

## Key API

- `detect_host_mismatch(baseline, current)` — returns `Option<HostMismatchInfo>` with mismatch reasons

## Example

```rust
use perfgate_host_detect::detect_host_mismatch;
use perfgate_types::HostInfo;

let baseline = HostInfo {
    os: "linux".to_string(),
    arch: "x86_64".to_string(),
    cpu_count: Some(8),
    memory_bytes: Some(16 * 1024 * 1024 * 1024),
    hostname_hash: Some("abc123".to_string()),
};

let current = HostInfo {
    os: "linux".to_string(),
    arch: "x86_64".to_string(),
    cpu_count: Some(8),
    memory_bytes: Some(16 * 1024 * 1024 * 1024),
    hostname_hash: Some("abc123".to_string()),
};

assert!(detect_host_mismatch(&baseline, &current).is_none());
```

## License

Licensed under either Apache-2.0 or MIT.
