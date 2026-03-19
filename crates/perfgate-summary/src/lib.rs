//! Summarization logic for perfgate comparison receipts.

use anyhow::Context;
use glob::glob;
use perfgate_types::{CompareReceipt, Metric};
use std::fs;

/// Request for summarizing multiple comparison receipts.
#[derive(Debug, Clone)]
pub struct SummaryRequest {
    /// List of glob patterns or file paths.
    pub files: Vec<String>,
}

/// A single row in the summary table.
#[derive(Debug, Clone)]
pub struct SummaryRow {
    pub benchmark: String,
    pub status: String,
    pub wall_ms: String,
    pub change_pct: String,
}

/// Outcome of the summary operation.
#[derive(Debug, Clone)]
pub struct SummaryOutcome {
    pub rows: Vec<SummaryRow>,
}

/// Use case for summarizing comparison receipts.
pub struct SummaryUseCase;

impl SummaryUseCase {
    /// Executes the summary use case.
    pub fn execute(&self, req: SummaryRequest) -> anyhow::Result<SummaryOutcome> {
        println!("DEBUG: SummaryUseCase::execute");
        let mut paths = Vec::new();
        for pattern in req.files {
            println!("DEBUG: matching glob {}", pattern);
            for entry in
                glob(&pattern).with_context(|| format!("invalid glob pattern: {}", pattern))?
            {
                paths.push(entry?);
            }
        }

        if paths.is_empty() {
            anyhow::bail!("no comparison receipts found");
        }

        let mut rows = Vec::new();
        for path in paths {
            println!("DEBUG: processing {}", path.display());
            let content =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            println!("DEBUG: content read ({} bytes)", content.len());
            let compare: CompareReceipt = serde_json::from_str(&content)
                .with_context(|| format!("parse JSON from {}", path.display()))?;
            println!("DEBUG: JSON parsed");

            let benchmark = compare.bench.name.clone();
            let status = format!("{:?}", compare.verdict.status).to_lowercase();
            println!("DEBUG: benchmark={}, status={}", benchmark, status);

            let wall = compare.deltas.get(&Metric::WallMs);
            let (wall_ms, change_pct) = if let Some(d) = wall {
                (
                    format!("{:.2}", d.current),
                    format!("{:.1}%", d.pct * 100.0),
                )
            } else {
                ("N/A".to_string(), "N/A".to_string())
            };

            rows.push(SummaryRow {
                benchmark,
                status,
                wall_ms,
                change_pct,
            });
        }

        Ok(SummaryOutcome { rows })
    }

    /// Renders the summary outcome as a Markdown table.
    pub fn render_markdown(&self, outcome: &SummaryOutcome) -> String {
        let mut md = String::new();
        md.push_str("\n| Benchmark | Status | Wall (ms) | Change |\n");
        md.push_str("|-----------|--------|-----------|--------|\n");

        for row in &outcome.rows {
            md.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                row.benchmark, row.status, row.wall_ms, row.change_pct
            ));
        }
        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, CompareReceipt, CompareRef, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn test_summary_execution() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("run1.json");

        let receipt = CompareReceipt {
            schema: "perfgate.compare.v1".to_string(),
            tool: ToolInfo {
                name: "test".into(),
                version: "0".into(),
            },
            bench: BenchMeta {
                name: "bench1".into(),
                cwd: None,
                command: vec![],
                repeat: 0,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets: BTreeMap::new(),
            deltas: BTreeMap::new(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
        };

        fs::write(&path, serde_json::to_string(&receipt).unwrap()).unwrap();

        let usecase = SummaryUseCase;
        let outcome = usecase
            .execute(SummaryRequest {
                files: vec![path.to_str().unwrap().to_string()],
            })
            .unwrap();

        assert_eq!(outcome.rows.len(), 1);
        assert_eq!(outcome.rows[0].benchmark, "bench1");
        assert_eq!(outcome.rows[0].status, "pass");
    }
}
