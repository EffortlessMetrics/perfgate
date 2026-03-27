# perfgate-render

Human-readable output from performance comparisons.

Takes a `CompareReceipt` and turns it into something a human can read --
a Markdown table for a PR comment, GitHub Actions annotations for the
checks tab, or a fully custom layout via Handlebars templates.

## Output Formats

| Function | Output | Where it shows up |
|----------------------------|-------------------------------|----------------------------------|
| `render_markdown` | Markdown table with verdicts | PR comments, CI summaries |
| `github_annotations` | `::error::` / `::warning::` | GitHub Actions checks tab |
| `render_markdown_template` | Custom Handlebars template | Anywhere you need a custom layout |

## Quick Start

```rust
use perfgate_render::{render_markdown, github_annotations, render_markdown_template};

// Markdown table for a PR comment
let md = render_markdown(&compare_receipt);

// GitHub Actions annotations (only emitted for warn/fail metrics)
let annotations = github_annotations(&compare_receipt);

// Custom template
let tmpl = "{{header}}\n{{#each rows}}- {{metric}}: {{delta_pct}}\n{{/each}}";
let custom = render_markdown_template(&compare_receipt, tmpl)?;
```

## Helpers

Formatting functions used internally, also exported for custom renderers:

| Function | Example output |
|-------------------------------|--------------------------|
| `format_value(metric, v)` | `"42"`, `"1.234"` |
| `format_pct(pct)` | `"+10.00%"`, `"-3.50%"` |
| `direction_str(dir)` | `"lower"`, `"higher"` |
| `metric_status_icon(status)` | pass / warn / fail / skip |
| `metric_status_str(status)` | `"pass"`, `"warn"` |
| `parse_reason_token(token)` | `"wall_ms_warn"` -> `(WallMs, Warn)` |

## Template Context

`render_markdown_template` exposes a JSON context with `header`, `bench`,
`verdict`, `rows` (array of per-metric objects with `metric`, `delta_pct`,
`status_icon`, `raw`, etc.), `reasons`, and the full `compare` receipt.

## License

Licensed under either Apache-2.0 or MIT.
