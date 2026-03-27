# perfgate-export

Get performance data out of perfgate and into your existing tools.

Benchmarks produce structured receipts. This crate converts them into
formats your dashboards, alerting pipelines, and CI systems already
understand -- one function call, one format enum.

## Supported Formats

| Format | Extension | Use case |
|-----------|-----------|-------------------------------------------|
| CSV | `.csv` | Spreadsheets, pandas, ad-hoc analysis |
| JSONL | `.jsonl` | Log aggregation (ELK, Loki, Datadog) |
| HTML | `.html` | Embeddable summary tables for reports |
| Prometheus | `.prom` | Pushgateway / time-series ingestion |
| JUnit | `.xml` | Legacy CI reporters (Jenkins, GitLab) |

All formats are available for both **run receipts** (raw samples) and
**compare receipts** (baseline vs. current deltas).

## API

The entire surface is two functions and one enum:

```rust
use perfgate_export::{ExportFormat, ExportUseCase};

// Pick a format
let fmt = ExportFormat::parse("csv").unwrap(); // Csv | Jsonl | Html | Prometheus | JUnit

// Export a run receipt
let csv = ExportUseCase::export_run(&run_receipt, fmt)?;

// Export a compare receipt
let prom = ExportUseCase::export_compare(&compare_receipt, ExportFormat::Prometheus)?;
```

### Types

| Item | Description |
|---------------------|----------------------------------------------|
| `ExportFormat` | Enum of supported output formats |
| `ExportUseCase` | Stateless exporter with `export_run` / `export_compare` |
| `RunExportRow` | Typed row for run receipt exports |
| `CompareExportRow` | Typed row for compare receipt exports |

## CLI

```bash
perfgate export --run out.json --format csv --out data.csv
perfgate export --run out.json --format prometheus --out metrics.prom
```

## License

Licensed under either Apache-2.0 or MIT.
