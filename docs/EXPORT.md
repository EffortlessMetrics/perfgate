# Exporting Data

perfgate can export run and comparison data to multiple formats for trend
analysis, dashboards, and CI integration.

## Formats

| Format | Extension | Use case |
|--------|-----------|----------|
| CSV | `.csv` | Spreadsheets, pandas, SQL import |
| JSONL | `.jsonl` | Structured log pipelines, streaming |
| HTML | `.html` | Standalone summary pages |
| Prometheus | `.prom` | Prometheus text exposition format |
| JUnit | `.xml` | Legacy CI reporters (Jenkins, etc.) |

## Usage

Export from a run receipt:

```bash
perfgate export --run run.json --format csv --out data.csv
perfgate export --run run.json --format jsonl --out data.jsonl
```

Export from a comparison receipt:

```bash
perfgate export --compare compare.json --format html --out summary.html
perfgate export --compare compare.json --format prometheus --out metrics.prom
perfgate export --compare compare.json --format junit --out results.xml
```

## Trend Analysis

For long-term drift analysis, export to JSONL or Prometheus from your nightly CI
lane and feed into your observability stack. The nightly lane in perfgate's own
self-dogfooding CI does exactly this — see [SELF_DOGFOODING.md](SELF_DOGFOODING.md).
