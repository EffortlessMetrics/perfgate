# perfgate Template Hub

This directory contains community-contributed Handlebars templates for rendering custom markdown reports from `CompareReceipt`s.

## Usage

You can use these templates with the `perfgate md` command to customize how the markdown comment is generated for your pull requests.

```bash
perfgate md --compare artifacts/perfgate/compare.json --template templates/compact.hbs
```

## Available Templates

- `default.hbs`: A replica of the default perfgate output, providing a full table with budgets and reasons.
- `compact.hbs`: A minimalist table showing only the metric, icon, and percentage delta.

## Writing Your Own

Templates use [Handlebars](https://docs.rs/handlebars).
When a template is rendered, the following JSON context is available:

```json
{
  "header": "✅ perfgate: pass",
  "bench": { /* BenchMeta object */ },
  "verdict": { /* Verdict object */ },
  "rows": [
    {
      "metric": "wall_ms",
      "metric_with_statistic": "wall_ms (median)",
      "statistic": "median",
      "baseline": "100 ms",
      "current": "110 ms",
      "unit": "ms",
      "delta_pct": "+10.00%",
      "budget_threshold_pct": 20.0,
      "budget_direction": "<",
      "status": "pass",
      "status_icon": "✅",
      "raw": { /* Raw delta numbers */ }
    }
  ],
  "reasons": ["wall_ms_warn"]
}
```
