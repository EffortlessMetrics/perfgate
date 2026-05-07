# perfgate-sensor

Compatibility wrapper for perfgate sensor report APIs during the 0.16
public-surface migration.

New code should use the facade package:

```rust
use perfgate::presentation::sensor::SensorReportBuilder;

let report = SensorReportBuilder::new(tool_info, started_at).build(&perfgate_report);
```

The wrapper remains available inside the workspace for transition compatibility,
but it is not part of the intended public package surface.
