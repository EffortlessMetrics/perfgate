# perfgate-export

Export formats for perfgate benchmarks.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

Provides functionality for exporting run and compare receipts to various
formats suitable for trend analysis and time-series ingestion.

## Supported Formats

| Format       | Description                                  |
|--------------|----------------------------------------------|
| CSV          | RFC 4180 compliant with header row           |
| JSONL        | JSON Lines (one JSON object per line)        |
| HTML         | HTML summary table                           |
| Prometheus   | Prometheus text exposition format             |
| JUnit        | JUnit XML for legacy CI reporters             |

## Key API

- `ExportFormat` — enum of supported formats (Csv, Jsonl, Html, Prometheus, JUnit)
- `ExportFormat::parse(s)` — parse format from string
- `ExportUseCase::export_run(receipt, format)` — export a run receipt
- `ExportUseCase::export_compare(receipt, format)` — export a compare receipt
- `RunExportRow` / `CompareExportRow` — typed row structures

## Example

```rust,ignore
use perfgate_export::{ExportFormat, ExportUseCase};

// Export a run receipt to CSV
let csv = ExportUseCase::export_run(&run_receipt, ExportFormat::Csv)?;

// Export a compare receipt to Prometheus format
let prom = ExportUseCase::export_compare(&compare_receipt, ExportFormat::Prometheus)?;
```

## License

Licensed under either Apache-2.0 or MIT.
