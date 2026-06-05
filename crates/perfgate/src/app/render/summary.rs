//! Summarization logic for perfgate comparison receipts.
//!
//! Aggregates multiple comparison receipts into a compact summary table showing
//! benchmark name, verdict status, wall-clock time, and percentage change.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Example
//!
//! ```no_run
//! use perfgate::app::render::summary::{SummaryRequest, SummaryUseCase};
//!
//! let uc = SummaryUseCase;
//! let outcome = uc.execute(SummaryRequest {
//!     files: vec!["artifacts/perfgate/*.compare.json".to_string()],
//! }).unwrap();
//! for row in &outcome.rows {
//!     println!("{}: {} ({})", row.benchmark, row.status, row.change_pct);
//! }
//! ```

use anyhow::Context;
use glob::glob;
use perfgate_types::{CompareReceipt, Metric, REPORT_SCHEMA_V1};
use std::fs;
use std::path::Path;

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
    pub failed: bool,
}

/// Explains why compare receipts are absent when sibling reports prove no baseline exists.
pub fn no_baseline_compare_receipts_message(target: &str) -> String {
    let target = if target.trim().is_empty() {
        "<none>"
    } else {
        target
    };
    format!(
        "No compare receipts found at {target}. If this is the first perfgate check, compare.json is not written until a baseline exists. Inspect run.json, report.json, and comment.md. If the run is representative, run `perfgate baseline promote --config perfgate.toml --all`, then rerun `perfgate check --config perfgate.toml --all --require-baseline`."
    )
}

/// Returns first-run guidance only when a sibling report proves baseline-less check output.
pub fn no_baseline_compare_receipts_message_for_path(path: &Path) -> Option<String> {
    if path.file_name().and_then(|name| name.to_str()) != Some("compare.json") {
        return None;
    }
    let report = path.with_file_name("report.json");
    report_is_no_baseline(&report)
        .then(|| no_baseline_compare_receipts_message(&path.display().to_string()))
}

fn no_baseline_compare_receipts_message_for_patterns(patterns: &[String]) -> Option<String> {
    if patterns.is_empty() {
        return None;
    }
    if !patterns
        .iter()
        .all(|pattern| pattern_has_only_no_baseline_reports(pattern))
    {
        return None;
    }
    Some(no_baseline_compare_receipts_message(&patterns.join(", ")))
}

fn pattern_has_only_no_baseline_reports(pattern: &str) -> bool {
    let Some(report_pattern) = pattern.strip_suffix("compare.json") else {
        return false;
    };
    let report_pattern = format!("{report_pattern}report.json");
    let Ok(entries) = glob(&report_pattern) else {
        return false;
    };

    let mut saw_report = false;
    for entry in entries {
        let Ok(path) = entry else {
            return false;
        };
        saw_report = true;
        if !report_is_no_baseline(&path) {
            return false;
        }
    }
    saw_report
}

fn report_is_no_baseline(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(report) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    let is_report =
        report.get("report_type").and_then(|value| value.as_str()) == Some(REPORT_SCHEMA_V1);
    let compare_absent = report.get("compare").is_none_or(|value| value.is_null());
    let has_no_baseline_reason = report
        .get("verdict")
        .and_then(|verdict| verdict.get("reasons"))
        .and_then(|reasons| reasons.as_array())
        .is_some_and(|reasons| {
            reasons
                .iter()
                .any(|reason| reason.as_str() == Some("no_baseline"))
        });
    let has_missing_baseline_finding = report
        .get("findings")
        .and_then(|findings| findings.as_array())
        .is_some_and(|findings| {
            findings.iter().any(|finding| {
                finding.get("check_id").and_then(|value| value.as_str()) == Some("perf.baseline")
                    && finding.get("code").and_then(|value| value.as_str()) == Some("missing")
            })
        });

    is_report && compare_absent && has_no_baseline_reason && has_missing_baseline_finding
}

/// Use case for summarizing comparison receipts.
pub struct SummaryUseCase;

impl SummaryUseCase {
    /// Executes the summary use case.
    pub fn execute(&self, req: SummaryRequest) -> anyhow::Result<SummaryOutcome> {
        let mut paths = Vec::new();
        for pattern in &req.files {
            for entry in
                glob(pattern).with_context(|| format!("invalid glob pattern: {}", pattern))?
            {
                paths.push(entry?);
            }
        }

        if paths.is_empty() {
            let message = no_baseline_compare_receipts_message_for_patterns(&req.files)
                .unwrap_or_else(|| "no comparison receipts found".to_string());
            anyhow::bail!("{message}");
        }

        let mut failed = false;
        let mut rows = Vec::new();
        for path in paths {
            let content =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let compare: CompareReceipt = serde_json::from_str(&content)
                .with_context(|| format!("parse JSON from {}", path.display()))?;

            let benchmark = compare.bench.name.clone();
            let status = format!("{:?}", compare.verdict.status).to_lowercase();
            let wall = compare.deltas.get(&Metric::WallMs);
            let (wall_ms, change_pct) = if let Some(d) = wall {
                (
                    format!("{:.2}", d.current),
                    format!("{:.1}%", d.pct * 100.0),
                )
            } else {
                ("N/A".to_string(), "N/A".to_string())
            };

            let row = rows.push_mut(SummaryRow {
                benchmark,
                status,
                wall_ms,
                change_pct,
            });
            if row.status == "fail" {
                failed = true;
            }
        }

        Ok(SummaryOutcome { rows, failed })
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
    use std::path::Path;
    use tempfile::tempdir;

    fn write_no_baseline_report(path: &Path) -> anyhow::Result<()> {
        fs::write(
            path,
            serde_json::json!({
                "report_type": "perfgate.report.v1",
                "verdict": {
                    "status": "warn",
                    "counts": { "pass": 0, "warn": 1, "fail": 0, "skip": 0 },
                    "reasons": ["no_baseline"]
                },
                "findings": [{
                    "check_id": "perf.baseline",
                    "code": "missing",
                    "severity": "warn",
                    "message": "No baseline found for bench 'bench'; comparison skipped"
                }],
                "summary": { "pass_count": 0, "warn_count": 1, "fail_count": 0, "skip_count": 0, "total_count": 1 }
            })
            .to_string(),
        )?;
        Ok(())
    }

    fn assert_no_baseline_guidance(message: &str) {
        assert!(message.contains("No compare receipts found"));
        assert!(message.contains("compare.json is not written until a baseline exists"));
        assert!(message.contains("run.json"));
        assert!(message.contains("report.json"));
        assert!(message.contains("comment.md"));
        assert!(message.contains("perfgate baseline promote --config perfgate.toml --all"));
    }

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
                    warn: 1,
                    fail: 0,
                    skip: 0,
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

    #[test]
    fn missing_literal_compare_with_no_baseline_report_explains_bootstrap() -> anyhow::Result<()> {
        let dir = tempdir()?;
        write_no_baseline_report(&dir.path().join("report.json"))?;

        let error = match SummaryUseCase.execute(SummaryRequest {
            files: vec![
                dir.path()
                    .join("compare.json")
                    .to_string_lossy()
                    .into_owned(),
            ],
        }) {
            Ok(_) => anyhow::bail!("expected missing compare receipts error"),
            Err(error) => error.to_string(),
        };

        assert_no_baseline_guidance(&error);
        Ok(())
    }

    #[test]
    fn missing_glob_compare_with_no_baseline_reports_explains_bootstrap() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let bench_dir = dir.path().join("artifacts").join("bench-a");
        fs::create_dir_all(&bench_dir)?;
        write_no_baseline_report(&bench_dir.join("report.json"))?;

        let root = dir.path().to_string_lossy().replace('\\', "/");
        let error = match SummaryUseCase.execute(SummaryRequest {
            files: vec![format!("{root}/artifacts/*/compare.json")],
        }) {
            Ok(_) => anyhow::bail!("expected missing compare receipts error"),
            Err(error) => error.to_string(),
        };

        assert_no_baseline_guidance(&error);
        Ok(())
    }

    #[test]
    fn arbitrary_empty_glob_keeps_generic_missing_message() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let root = dir.path().to_string_lossy().replace('\\', "/");

        let error = match SummaryUseCase.execute(SummaryRequest {
            files: vec![format!("{root}/missing/*/compare.json")],
        }) {
            Ok(_) => anyhow::bail!("expected missing compare receipts error"),
            Err(error) => error.to_string(),
        };

        assert_eq!(error, "no comparison receipts found");
        Ok(())
    }
}
