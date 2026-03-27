//! Trend analysis use case.
//!
//! Analyzes metric history from a sequence of run receipts to detect drift
//! and predict budget threshold breaches.

use perfgate_domain::{
    DriftClass, TrendAnalysis, TrendConfig, analyze_trend, metric_value, spark_chart,
};
use perfgate_types::{Direction, Metric, RunReceipt};

/// Request for local trend analysis from run receipt files.
#[derive(Debug, Clone)]
pub struct TrendRequest {
    /// Run receipts in chronological order.
    pub history: Vec<RunReceipt>,
    /// Budget threshold as a fraction (e.g., 0.20 for 20%).
    /// Applied relative to the first run's metric value.
    pub threshold: f64,
    /// Specific metric to analyze (if None, analyze all available metrics).
    pub metric: Option<Metric>,
    /// Trend classification config.
    pub config: TrendConfig,
}

/// Result of trend analysis.
#[derive(Debug, Clone)]
pub struct TrendOutcome {
    /// Per-metric trend analyses.
    pub analyses: Vec<TrendAnalysis>,
    /// Benchmark name (from first receipt).
    pub bench_name: String,
    /// Number of runs analyzed.
    pub run_count: usize,
}

/// Use case for trend analysis.
pub struct TrendUseCase;

impl TrendUseCase {
    /// Execute trend analysis on a series of run receipts.
    pub fn execute(&self, request: TrendRequest) -> anyhow::Result<TrendOutcome> {
        if request.history.is_empty() {
            anyhow::bail!("no run receipts provided for trend analysis");
        }

        let bench_name = request.history[0].bench.name.clone();
        let run_count = request.history.len();

        let metrics_to_analyze = if let Some(m) = request.metric {
            vec![m]
        } else {
            available_metrics(&request.history)
        };

        let mut analyses = Vec::new();

        for metric in &metrics_to_analyze {
            let values: Vec<f64> = request
                .history
                .iter()
                .filter_map(|run| metric_value(&run.stats, *metric))
                .collect();

            if values.len() < 2 {
                continue;
            }

            // Compute absolute threshold from the first run's value and the relative threshold
            let baseline_value = values[0];
            let direction = metric.default_direction();
            let lower_is_better = direction == Direction::Lower;

            let absolute_threshold = if lower_is_better {
                baseline_value * (1.0 + request.threshold)
            } else {
                baseline_value * (1.0 - request.threshold)
            };

            if let Some(analysis) = analyze_trend(
                &values,
                metric.as_str(),
                absolute_threshold,
                lower_is_better,
                &request.config,
            ) {
                analyses.push(analysis);
            }
        }

        Ok(TrendOutcome {
            analyses,
            bench_name,
            run_count,
        })
    }
}

/// Determine which metrics have data across the run history.
fn available_metrics(runs: &[RunReceipt]) -> Vec<Metric> {
    let all_metrics = [
        Metric::WallMs,
        Metric::CpuMs,
        Metric::MaxRssKb,
        Metric::PageFaults,
        Metric::CtxSwitches,
        Metric::IoReadBytes,
        Metric::IoWriteBytes,
        Metric::NetworkPackets,
        Metric::EnergyUj,
        Metric::BinaryBytes,
        Metric::ThroughputPerS,
    ];

    all_metrics
        .into_iter()
        .filter(|m| {
            // A metric is available if at least 2 runs have data for it
            let count = runs
                .iter()
                .filter(|r| metric_value(&r.stats, *m).is_some())
                .count();
            count >= 2
        })
        .collect()
}

/// Format trend analysis results for terminal display.
pub fn format_trend_output(outcome: &TrendOutcome) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "Trend Analysis: {} ({} runs)\n",
        outcome.bench_name, outcome.run_count
    ));
    out.push_str(&"=".repeat(60));
    out.push('\n');

    if outcome.analyses.is_empty() {
        out.push_str("No trend data available (need at least 2 data points per metric).\n");
        return out;
    }

    for analysis in &outcome.analyses {
        let icon = match analysis.drift {
            DriftClass::Stable => "[OK]",
            DriftClass::Improving => "[++]",
            DriftClass::Degrading => "[!!]",
            DriftClass::Critical => "[XX]",
        };

        out.push_str(&format!("\n{} {}\n", icon, analysis.metric));
        out.push_str(&format!("  Drift:     {}\n", analysis.drift));
        out.push_str(&format!(
            "  Slope:     {:+.4}/run\n",
            analysis.slope_per_run
        ));
        out.push_str(&format!("  R-squared: {:.4}\n", analysis.r_squared));
        out.push_str(&format!(
            "  Headroom:  {:.1}%\n",
            analysis.current_headroom_pct
        ));

        if let Some(runs) = analysis.runs_to_breach {
            out.push_str(&format!("  Breach in: ~{} runs\n", runs));
        }
    }

    out
}

/// Format a mini ASCII chart line for a metric's history.
pub fn format_trend_chart(values: &[f64], metric_name: &str) -> String {
    let chart = spark_chart(values);
    format!("  {} [{}]", metric_name, chart)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, HostInfo, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
    };

    fn make_run(name: &str, wall_median: u64) -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "test".to_string(),
            },
            run: RunMeta {
                id: format!("run-{}", wall_median),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: name.to_string(),
                cwd: None,
                command: vec!["echo".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: wall_median,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary {
                    median: wall_median,
                    min: wall_median,
                    max: wall_median,
                    mean: Some(wall_median as f64),
                    stddev: Some(0.0),
                },
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
        }
    }

    #[test]
    fn trend_usecase_degrading() {
        let history = vec![
            make_run("bench-a", 100),
            make_run("bench-a", 105),
            make_run("bench-a", 110),
            make_run("bench-a", 115),
            make_run("bench-a", 120),
        ];

        let request = TrendRequest {
            history,
            threshold: 0.30,
            metric: Some(Metric::WallMs),
            config: TrendConfig::default(),
        };

        let outcome = TrendUseCase.execute(request).unwrap();
        assert_eq!(outcome.bench_name, "bench-a");
        assert_eq!(outcome.run_count, 5);
        assert_eq!(outcome.analyses.len(), 1);

        let a = &outcome.analyses[0];
        assert_eq!(a.metric, "wall_ms");
        assert!(matches!(
            a.drift,
            DriftClass::Degrading | DriftClass::Critical
        ));
    }

    #[test]
    fn trend_usecase_empty_history() {
        let request = TrendRequest {
            history: vec![],
            threshold: 0.20,
            metric: None,
            config: TrendConfig::default(),
        };

        assert!(TrendUseCase.execute(request).is_err());
    }

    #[test]
    fn trend_usecase_single_run() {
        let request = TrendRequest {
            history: vec![make_run("bench-a", 100)],
            threshold: 0.20,
            metric: Some(Metric::WallMs),
            config: TrendConfig::default(),
        };

        let outcome = TrendUseCase.execute(request).unwrap();
        // Single run => not enough data points, so no analyses
        assert!(outcome.analyses.is_empty());
    }

    #[test]
    fn format_trend_output_basic() {
        let outcome = TrendOutcome {
            analyses: vec![TrendAnalysis {
                metric: "wall_ms".to_string(),
                slope_per_run: 2.5,
                intercept: 100.0,
                r_squared: 0.95,
                drift: DriftClass::Degrading,
                runs_to_breach: Some(8),
                current_headroom_pct: 15.0,
                sample_count: 5,
            }],
            bench_name: "my-bench".to_string(),
            run_count: 5,
        };

        let text = format_trend_output(&outcome);
        assert!(text.contains("my-bench"));
        assert!(text.contains("5 runs"));
        assert!(text.contains("wall_ms"));
        assert!(text.contains("degrading"));
        assert!(text.contains("~8 runs"));
    }
}
