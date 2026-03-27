# perfgate-host-detect

Comparing benchmarks from different hardware produces misleading results.
This crate provides host fingerprinting and mismatch detection so perfgate
can warn you before bad comparisons happen.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## How it works

`detect_host_mismatch` compares two `HostInfo` snapshots and returns
`Some(HostMismatchInfo)` when any signal exceeds its threshold:

| Signal       | Threshold                | Example                  |
|--------------|--------------------------|--------------------------|
| OS           | Any difference           | `linux` vs `windows`     |
| Architecture | Any difference           | `x86_64` vs `aarch64`   |
| CPU count    | > 2x ratio               | 4 vs 16 CPUs            |
| Memory       | > 2x ratio               | 8 GB vs 64 GB           |
| Hostname hash| Different (both present) | Different machines       |

The 2x threshold avoids false positives from minor variations (8 vs 10 CPUs)
while catching meaningful differences (4 vs 32 CPUs) that skew results.
Optional fields that are `None` on either side are silently skipped.

## Key API

- `detect_host_mismatch(baseline, current) -> Option<HostMismatchInfo>`

## Example

```rust
use perfgate_host_detect::detect_host_mismatch;
use perfgate_types::HostInfo;

let baseline = HostInfo {
    os: "linux".into(), arch: "x86_64".into(),
    cpu_count: Some(8), memory_bytes: Some(16 * 1024 * 1024 * 1024),
    hostname_hash: Some("abc123".into()),
};

// Same machine -- no mismatch
assert!(detect_host_mismatch(&baseline, &baseline).is_none());
```

## License

Licensed under either Apache-2.0 or MIT.
