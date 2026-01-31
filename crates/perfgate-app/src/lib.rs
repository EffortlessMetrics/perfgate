//! Application layer for perfgate.
//!
//! The app layer coordinates adapters and domain logic.
//! It does not parse CLI flags and it does not do filesystem I/O.

use anyhow::Context;
use perfgate_adapters::{CommandSpec, ProcessRunner, RunResult};
use perfgate_domain::{compare_stats, compute_stats, Comparison};
use perfgate_types::{
    BenchMeta, Budget, CompareReceipt, CompareRef, Delta, Direction, HostInfo, Metric, MetricStatus,
    RunMeta, RunReceipt, Sample, ToolInfo, Verdict,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

#[derive(Debug, Default, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        use time::format_description::well_known::Rfc3339;
        time::OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct RunBenchRequest {
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub command: Vec<String>,
    pub repeat: u32,
    pub warmup: u32,
    pub work_units: Option<u64>,
    pub timeout: Option<Duration>,
    pub env: Vec<(String, String)>,
    pub output_cap_bytes: usize,

    /// If true, do not treat nonzero exit codes as a tool error.
    /// The receipt will still record exit codes.
    pub allow_nonzero: bool,
}

#[derive(Debug, Clone)]
pub struct RunBenchOutcome {
    pub receipt: RunReceipt,

    /// True if any measured (non-warmup) sample timed out or returned nonzero.
    pub failed: bool,

    /// Human-readable reasons (for CI logs).
    pub reasons: Vec<String>,
}

pub struct RunBenchUseCase<R: ProcessRunner, C: Clock> {
    runner: R,
    clock: C,
    tool: ToolInfo,
}

impl<R: ProcessRunner, C: Clock> RunBenchUseCase<R, C> {
    pub fn new(runner: R, clock: C, tool: ToolInfo) -> Self {
        Self { runner, clock, tool }
    }

    pub fn execute(&self, req: RunBenchRequest) -> anyhow::Result<RunBenchOutcome> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let started_at = self.clock.now_rfc3339();

        let host = HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        };

        let bench = BenchMeta {
            name: req.name.clone(),
            cwd: req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            command: req.command.clone(),
            repeat: req.repeat,
            warmup: req.warmup,
            work_units: req.work_units,
            timeout_ms: req.timeout.map(|d| d.as_millis() as u64),
        };

        let mut samples: Vec<Sample> = Vec::new();
        let mut reasons: Vec<String> = Vec::new();

        let total = req.warmup + req.repeat;

        for i in 0..total {
            let is_warmup = i < req.warmup;

            let spec = CommandSpec {
                argv: req.command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };

            let run = self
                .runner
                .run(&spec)
                .with_context(|| format!("failed to run command (iteration {}): {:?}", i + 1, spec.argv))?;

            let s = sample_from_run(run, is_warmup);
            if !is_warmup {
                if s.timed_out {
                    reasons.push(format!("iteration {} timed out", i + 1));
                }
                if s.exit_code != 0 {
                    reasons.push(format!("iteration {} exit code {}", i + 1, s.exit_code));
                }
            }

            samples.push(s);
        }

        let stats = compute_stats(&samples, req.work_units)?;

        let ended_at = self.clock.now_rfc3339();

        let receipt = RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: self.tool.clone(),
            run: RunMeta {
                id: run_id,
                started_at,
                ended_at,
                host,
            },
            bench,
            samples,
            stats,
        };

        let failed = !reasons.is_empty();

        if failed && !req.allow_nonzero {
            // It's still a successful run from a *tooling* perspective, but callers may want a hard failure.
            // We return the receipt either way; the CLI decides exit codes.
        }

        Ok(RunBenchOutcome {
            receipt,
            failed,
            reasons,
        })
    }
}

fn sample_from_run(run: RunResult, warmup: bool) -> Sample {
    Sample {
        wall_ms: run.wall_ms,
        exit_code: run.exit_code,
        warmup,
        timed_out: run.timed_out,
        max_rss_kb: run.max_rss_kb,
        stdout: if run.stdout.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stdout).to_string())
        },
        stderr: if run.stderr.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stderr).to_string())
        },
    }
}

#[derive(Debug, Clone)]
pub struct CompareRequest {
    pub baseline: RunReceipt,
    pub current: RunReceipt,
    pub budgets: BTreeMap<Metric, Budget>,
    pub baseline_ref: CompareRef,
    pub current_ref: CompareRef,
    pub tool: ToolInfo,
}

pub struct CompareUseCase;

impl CompareUseCase {
    pub fn execute(req: CompareRequest) -> anyhow::Result<CompareReceipt> {
        let Comparison { deltas, verdict } = compare_stats(&req.baseline.stats, &req.current.stats, &req.budgets)?;

        Ok(CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: req.tool,
            bench: req.current.bench,
            baseline_ref: req.baseline_ref,
            current_ref: req.current_ref,
            budgets: req.budgets,
            deltas,
            verdict,
        })
    }
}

// ----------------------------
// Rendering helpers
// ----------------------------

pub fn render_markdown(compare: &CompareReceipt) -> String {
    let mut out = String::new();

    let header = match compare.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate: fail",
    };

    out.push_str(header);
    out.push_str("\n\n");

    out.push_str(&format!(
        "**Bench:** `{}`\n\n",
        compare.bench.name
    ));

    out.push_str("| metric | baseline (median) | current (median) | delta | budget | status |\n");
    out.push_str("|---|---:|---:|---:|---:|---|\n");

    for (metric, delta) in &compare.deltas {
        let budget = compare.budgets.get(metric);
        let (budget_str, direction_str) = if let Some(b) = budget {
            (
                format!("{:.1}%", b.threshold * 100.0),
                match b.direction {
                    Direction::Lower => "lower",
                    Direction::Higher => "higher",
                },
            )
        } else {
            ("".to_string(), "")
        };

        let status_icon = match delta.status {
            MetricStatus::Pass => "✅",
            MetricStatus::Warn => "⚠️",
            MetricStatus::Fail => "❌",
        };

        out.push_str(&format!(
            "| `{metric}` | {b} {u} | {c} {u} | {pct} | {budget} ({dir}) | {status} |\n",
            metric = format_metric(*metric),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
            pct = format_pct(delta.pct),
            budget = budget_str,
            dir = direction_str,
            status = status_icon,
        ));
    }

    if !compare.verdict.reasons.is_empty() {
        out.push_str("\n**Notes:**\n");
        for r in &compare.verdict.reasons {
            out.push_str(&format!("- {}\n", r));
        }
    }

    out
}

pub fn github_annotations(compare: &CompareReceipt) -> Vec<String> {
    let mut lines = Vec::new();

    for (metric, delta) in &compare.deltas {
        let prefix = match delta.status {
            MetricStatus::Fail => "::error",
            MetricStatus::Warn => "::warning",
            MetricStatus::Pass => continue,
        };

        let msg = format!(
            "perfgate {bench} {metric}: {pct} (baseline {b}{u}, current {c}{u})",
            bench = compare.bench.name,
            metric = format_metric(*metric),
            pct = format_pct(delta.pct),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
        );

        lines.push(format!("{prefix}::{msg}"));
    }

    lines
}

fn format_metric(metric: Metric) -> &'static str {
    match metric {
        Metric::WallMs => "wall_ms",
        Metric::MaxRssKb => "max_rss_kb",
        Metric::ThroughputPerS => "throughput_per_s",
    }
}

fn format_value(metric: Metric, v: f64) -> String {
    match metric {
        Metric::WallMs | Metric::MaxRssKb => format!("{:.0}", v),
        Metric::ThroughputPerS => format!("{:.3}", v),
    }
}

fn format_pct(pct: f64) -> String {
    let sign = if pct > 0.0 { "+" } else { "" };
    format!("{}{:.2}%", sign, pct * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{F64Summary, U64Summary, VerdictCounts, VerdictStatus};

    #[test]
    fn markdown_renders_table() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget { threshold: 0.2, warn_threshold: 0.18, direction: Direction::Lower },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1100.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
                status: MetricStatus::Pass,
            },
        );

        let compare = CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
            bench: BenchMeta {
                name: "demo".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef { path: None, run_id: None },
            current_ref: CompareRef { path: None, run_id: None },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts { pass: 1, warn: 0, fail: 0 },
                reasons: vec![],
            },
        };

        let md = render_markdown(&compare);
        assert!(md.contains("| metric | baseline"));
        assert!(md.contains("wall_ms"));
    }
}
