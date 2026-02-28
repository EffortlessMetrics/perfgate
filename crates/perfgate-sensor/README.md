# perfgate-sensor

Sensor report building for cockpit integration.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Wraps `PerfgateReport` into a `sensor.report.v1` envelope suitable for CI/CD
cockpit systems. The envelope includes tool metadata, run metadata,
capabilities, verdicts, findings with fingerprints, and artifact links.

## Key API

- `SensorReportBuilder` — builder for constructing `SensorReport` envelopes
- `sensor_fingerprint(parts)` — SHA-256 fingerprint from semantic parts (pipe-joined, trimmed)
- `default_engine_capability()` — platform-aware engine capability detection

## Envelope Contents

- **Tool metadata**: name, version
- **Run metadata**: timestamps, duration
- **Capabilities**: baseline availability, engine features
- **Verdict**: pass/warn/fail with counts
- **Findings**: individual check results with stable fingerprints
- **Artifacts**: links to detailed reports

## Example

```rust
use perfgate_sensor::{sensor_fingerprint, default_engine_capability};
use perfgate_types::CapabilityStatus;

let fp = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);
assert_eq!(fp.len(), 64); // SHA-256 hex

let cap = default_engine_capability();
// Available on Unix, Unavailable on other platforms
```

## License

Licensed under either Apache-2.0 or MIT.
