# perfgate-export

Compatibility wrapper for perfgate export APIs during the 0.16 public-surface
migration.

New code should use the facade package:

```rust
use perfgate::presentation::export::{ExportFormat, ExportUseCase};

let fmt = ExportFormat::parse("csv").unwrap();
let csv = ExportUseCase::export_run(&run_receipt, fmt)?;
```

The wrapper remains available inside the workspace for transition compatibility,
but it is not part of the intended public package surface.
