//! Basic example demonstrating markdown and GitHub annotation rendering.
//!
//! Run with: cargo run -p perfgate-render --example basic

use perfgate_render::{
    format_metric_with_statistic, format_pct, format_value, github_annotations, render_markdown,
    render_markdown_template,
};
use perfgate_types::MetricStatus;
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, Delta, Direction, Metric,
    MetricStatistic, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

fn create_compare_receipt(status: MetricStatus) -> CompareReceipt {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.2, 0.15, Direction::Lower));

    let mut deltas = BTreeMap::new();
    deltas.insert(
        Metric::WallMs,
        Delta {
            baseline: 100.0,
            current: 118.0,
            ratio: 1.18,
            pct: 0.18,
            regression: 0.18,
            statistic: MetricStatistic::Median,
            significance: None,
            status,
        },
    );

    CompareReceipt {
        schema: COMPARE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        bench: BenchMeta {
            name: "my-benchmark".to_string(),
            cwd: None,
            command: vec!["./test".to_string()],
            repeat: 5,
            warmup: 1,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: Some("baseline.json".to_string()),
            run_id: None,
        },
        current_ref: CompareRef {
            path: Some("current.json".to_string()),
            run_id: None,
        },
        budgets,
        deltas,
        verdict: Verdict {
            status: if status == MetricStatus::Pass {
                VerdictStatus::Pass
            } else if status == MetricStatus::Warn {
                VerdictStatus::Warn
            } else {
                VerdictStatus::Fail
            },
            counts: VerdictCounts {
                pass: if status == MetricStatus::Pass { 1 } else { 0 },
                warn: if status == MetricStatus::Warn { 1 } else { 0 },
                fail: if status == MetricStatus::Fail { 1 } else { 0 },
            },
            reasons: if status != MetricStatus::Pass {
                vec![format!("wall_ms_{}", status.as_str())]
            } else {
                vec![]
            },
        },
    }
}

fn main() {
    println!("=== perfgate-render Basic Example ===\n");

    println!("1. Rendering markdown for PASS result:");
    let pass_receipt = create_compare_receipt(MetricStatus::Pass);
    println!("{}", render_markdown(&pass_receipt));

    println!("2. Rendering markdown for WARN result:");
    let warn_receipt = create_compare_receipt(MetricStatus::Warn);
    println!("{}", render_markdown(&warn_receipt));

    println!("3. Rendering markdown for FAIL result:");
    let fail_receipt = create_compare_receipt(MetricStatus::Fail);
    println!("{}", render_markdown(&fail_receipt));

    println!("4. GitHub annotations (only warn and fail):");
    let annotations = github_annotations(&warn_receipt);
    for ann in &annotations {
        println!("   {}", ann);
    }

    println!("\n5. Formatting values:");
    println!("   Wall time: {} ms", format_value(Metric::WallMs, 123.0));
    println!(
        "   Throughput: {} /s",
        format_value(Metric::ThroughputPerS, 1500.5)
    );
    println!("   Max RSS: {} KB", format_value(Metric::MaxRssKb, 2048.0));

    println!("\n6. Formatting percentages:");
    println!("   +10%: {}", format_pct(0.10));
    println!("   -5%: {}", format_pct(-0.05));
    println!("   +18.5%: {}", format_pct(0.185));

    println!("\n7. Metric with statistic:");
    println!(
        "   Median: {}",
        format_metric_with_statistic(Metric::WallMs, MetricStatistic::Median)
    );
    println!(
        "   P95: {}",
        format_metric_with_statistic(Metric::WallMs, MetricStatistic::P95)
    );

    println!("\n8. Using a custom markdown template:");
    let template = r#"{{header}}

Benchmark: {{bench.name}}
Status: {{verdict.status}}

{{#each rows}}
- {{metric}}: {{baseline}} -> {{current}} ({{delta_pct}})
{{/each}}
"#;
    match render_markdown_template(&warn_receipt, template) {
        Ok(rendered) => println!("{}", rendered),
        Err(e) => println!("Template error: {}", e),
    }

    println!("=== Example complete ===");
}
