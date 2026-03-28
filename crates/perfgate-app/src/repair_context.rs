use crate::CheckOutcome;
use perfgate_types::{
    ChangedFilesSummary, GitContext, MetricStatus, REPAIR_CONTEXT_SCHEMA_V1, RepairArtifacts,
    RepairContextReceipt, RepairMetricBreach, SpanIdentifiers, VerdictStatus,
    redact_sensitive_tokens,
};

#[derive(Debug, Clone, Default)]
pub struct RepairContextOptions {
    pub changed_files: Option<ChangedFilesSummary>,
    pub git: Option<GitContext>,
    pub spans: Option<SpanIdentifiers>,
}

pub fn build_repair_context(
    outcome: &CheckOutcome,
    options: RepairContextOptions,
) -> Option<RepairContextReceipt> {
    let compare = outcome.compare_receipt.as_ref()?;

    let breached_metrics = compare
        .deltas
        .iter()
        .filter_map(|(metric, delta)| {
            if !matches!(delta.status, MetricStatus::Warn | MetricStatus::Fail) {
                return None;
            }
            let budget = compare.budgets.get(metric)?;
            Some(RepairMetricBreach {
                metric: metric.as_str().to_string(),
                status: delta.status.as_str().to_string(),
                baseline: delta.baseline,
                current: delta.current,
                threshold: budget.threshold,
                warn_threshold: budget.warn_threshold,
                direction: budget.direction,
            })
        })
        .collect::<Vec<_>>();

    let mut recommended_next_commands = vec![
        format!(
            "perfgate blame --baseline {} --current {}",
            compare
                .baseline_ref
                .path
                .clone()
                .unwrap_or_else(|| "baseline.json".to_string()),
            compare
                .current_ref
                .path
                .clone()
                .unwrap_or_else(|| "artifacts/perfgate/run.json".to_string())
        ),
        format!(
            "perfgate paired --name {} --baseline-cmd \"<baseline command>\" --current-cmd \"<current command>\" --repeat 10 --out artifacts/perfgate/paired.json",
            compare.bench.name
        ),
        "perfgate bisect --good <good_sha> --bad HEAD --executable <bench_binary>".to_string(),
    ];
    if compare.verdict.status == VerdictStatus::Warn {
        recommended_next_commands.push(format!(
            "perfgate check --config perfgate.toml --bench {} --fail-on-warn",
            compare.bench.name
        ));
    }
    recommended_next_commands = recommended_next_commands
        .into_iter()
        .map(|cmd| redact_sensitive_tokens(&cmd))
        .collect();

    Some(RepairContextReceipt {
        schema: REPAIR_CONTEXT_SCHEMA_V1.to_string(),
        benchmark: compare.bench.name.clone(),
        verdict: compare.verdict.clone(),
        breached_metrics,
        artifacts: RepairArtifacts {
            compare_receipt_path: outcome
                .compare_path
                .as_ref()
                .map(|p| p.display().to_string()),
            report_path: outcome.report_path.display().to_string(),
        },
        profile_path: outcome.report.profile_path.clone(),
        changed_files: options.changed_files,
        git: options.git,
        spans: options.spans,
        recommended_next_commands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareReceipt, CompareRef, Delta, Direction, PerfgateReport,
        ReportSummary, ToolInfo, Verdict, VerdictCounts,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn build_repair_context_collects_warn_and_fail_metrics() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            perfgate_types::Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Warn,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            perfgate_types::Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 130.0,
                ratio: 1.3,
                pct: 0.3,
                regression: 0.3,
                cv: None,
                noise_threshold: None,
                statistic: perfgate_types::MetricStatistic::Median,
                status: MetricStatus::Fail,
                significance: None,
            },
        );

        let compare = CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "1.0.0".into(),
            },
            bench: BenchMeta {
                name: "bench-a".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("base.json".into()),
                run_id: None,
            },
            current_ref: CompareRef {
                path: Some("run.json".into()),
                run_id: None,
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 1,
                    skip: 0,
                },
                reasons: vec!["wall_ms_fail".into()],
            },
        };

        let outcome = CheckOutcome {
            run_receipt: perfgate_types::RunReceipt {
                schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
                tool: compare.tool.clone(),
                run: perfgate_types::RunMeta {
                    id: "id".into(),
                    started_at: "s".into(),
                    ended_at: "e".into(),
                    host: perfgate_types::HostInfo {
                        os: "linux".into(),
                        arch: "x86_64".into(),
                        cpu_count: None,
                        memory_bytes: None,
                        hostname_hash: None,
                    },
                },
                bench: compare.bench.clone(),
                samples: vec![],
                stats: perfgate_types::Stats {
                    wall_ms: perfgate_types::U64Summary::new(100, 100, 100),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                },
            },
            run_path: PathBuf::from("artifacts/perfgate/run.json"),
            compare_receipt: Some(compare),
            compare_path: Some(PathBuf::from("artifacts/perfgate/compare.json")),
            report: PerfgateReport {
                report_type: perfgate_types::REPORT_SCHEMA_V1.to_string(),
                verdict: Verdict {
                    status: VerdictStatus::Fail,
                    counts: VerdictCounts {
                        pass: 0,
                        warn: 0,
                        fail: 1,
                        skip: 0,
                    },
                    reasons: vec![],
                },
                compare: None,
                findings: vec![],
                summary: ReportSummary {
                    pass_count: 0,
                    warn_count: 0,
                    fail_count: 1,
                    skip_count: 0,
                    total_count: 1,
                },
                profile_path: Some("profiles/bench.svg".into()),
            },
            report_path: PathBuf::from("artifacts/perfgate/report.json"),
            markdown: String::new(),
            markdown_path: PathBuf::from("artifacts/perfgate/comment.md"),
            warnings: vec![],
            failed: true,
            exit_code: 2,
            suggest_paired: false,
        };

        let ctx = build_repair_context(&outcome, RepairContextOptions::default())
            .expect("repair context should exist");
        assert_eq!(ctx.schema, REPAIR_CONTEXT_SCHEMA_V1);
        assert_eq!(ctx.breached_metrics.len(), 1);
        assert_eq!(ctx.profile_path.as_deref(), Some("profiles/bench.svg"));
    }
}
