# perfgate-render

Rendering utilities for perfgate output (markdown, GitHub annotations).

## Overview

This crate provides functions for rendering performance comparison results as:
- Markdown tables with metrics, budgets, and status icons
- GitHub Actions workflow annotations (`::error::` and `::warning::`)
- Template-based markdown rendering using Handlebars

## Features

- **Markdown Tables**: Render `CompareReceipt` as formatted markdown tables with verdict status
- **GitHub Annotations**: Generate `::error::` and `::warning::` annotations for CI workflows
- **Template Rendering**: Custom Handlebars templates for flexible output formatting
- **Helper Functions**: Format metrics, values, percentages, and status indicators

## Usage

```rust
use perfgate_render::{render_markdown, github_annotations, render_markdown_template};
use perfgate_types::CompareReceipt;

// Render a markdown table
let markdown = render_markdown(&compare_receipt);

// Generate GitHub Actions annotations
let annotations = github_annotations(&compare_receipt);

// Use a custom template
let template = "{{header}}\n{{#each rows}}- {{metric}}: {{delta_pct}}\n{{/each}}";
let custom = render_markdown_template(&compare_receipt, template)?;
```

## API

### Main Functions

| Function | Description |
|----------|-------------|
| `render_markdown` | Render a `CompareReceipt` as a markdown table |
| `render_markdown_template` | Render using a custom Handlebars template |
| `github_annotations` | Generate GitHub Actions annotations for failed/warned metrics |

### Helper Functions

| Function | Description |
|----------|-------------|
| `format_metric` | Get the string representation of a `Metric` |
| `format_metric_with_statistic` | Format metric name with statistic type |
| `format_value` | Format a metric value with appropriate precision |
| `format_pct` | Format a percentage with sign (e.g., `+10.00%`) |
| `direction_str` | Get the string for a budget direction (`lower`/`higher`) |
| `metric_status_icon` | Get the emoji for a status (✅/⚠️/❌) |
| `metric_status_str` | Get the string for a status (`pass`/`warn`/`fail`) |
| `parse_reason_token` | Parse a reason token like `wall_ms_warn` |
| `render_reason_line` | Render a reason line with threshold info |
| `markdown_template_context` | Get the JSON context for template rendering |

## Template Context

When using `render_markdown_template`, the following context is available:

```json
{
  "header": "✅ perfgate: pass",
  "bench": { "name": "...", ... },
  "verdict": { "status": "pass", ... },
  "rows": [
    {
      "metric": "wall_ms",
      "metric_with_statistic": "wall_ms",
      "statistic": "median",
      "baseline": "100",
      "current": "110",
      "unit": "ms",
      "delta_pct": "+10.00%",
      "budget_threshold_pct": 20.0,
      "budget_direction": "lower",
      "status": "warn",
      "status_icon": "⚠️",
      "raw": { "baseline": 100.0, "current": 110.0, ... }
    }
  ],
  "reasons": ["wall_ms_warn"],
  "compare": { ... }
}
```

## Testing

```bash
cargo test -p perfgate-render
```

The crate includes:
- Unit tests for all formatting functions
- Property-based tests for markdown rendering completeness
- Property-based tests for GitHub annotation generation
- Snapshot tests using insta for output verification
