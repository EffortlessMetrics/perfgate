use crate::domain::budget::{aggregate_verdict, calculate_regression};
use perfgate_types::{
    BenchMeta, CompareRef, Delta, Direction, HostInfo, Metric, MetricStatistic, MetricStatus,
    PROBE_COMPARE_SCHEMA_V1, PROBE_SCHEMA_V1, ProbeCompareObservation, ProbeCompareReceipt,
    ProbeMetricValue, ProbeObservation, ProbeReceipt, ProbeScope, RunMeta, ToolInfo,
};
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub struct ProbeCompareRequest {
    pub baseline: ProbeReceipt,
    pub current: ProbeReceipt,
    pub baseline_ref: CompareRef,
    pub current_ref: CompareRef,
    pub tool: ToolInfo,
}

#[derive(Debug)]
pub struct ProbeCompareOutcome {
    pub receipt: ProbeCompareReceipt,
}

pub struct ProbeCompareUseCase;

impl ProbeCompareUseCase {
    pub fn compare(req: ProbeCompareRequest) -> anyhow::Result<ProbeCompareOutcome> {
        if req.baseline.schema != PROBE_SCHEMA_V1 {
            anyhow::bail!(
                "baseline probe receipt must use schema '{}', got '{}'",
                PROBE_SCHEMA_V1,
                req.baseline.schema
            );
        }
        if req.current.schema != PROBE_SCHEMA_V1 {
            anyhow::bail!(
                "current probe receipt must use schema '{}', got '{}'",
                PROBE_SCHEMA_V1,
                req.current.schema
            );
        }
        if req.baseline.probes.is_empty() {
            anyhow::bail!("baseline probe receipt has no probes");
        }
        if req.current.probes.is_empty() {
            anyhow::bail!("current probe receipt has no probes");
        }

        let baseline = summarize_probes(&req.baseline.probes);
        let current = summarize_probes(&req.current.probes);
        let mut probe_names: BTreeSet<String> = baseline.keys().cloned().collect();
        probe_names.extend(current.keys().cloned());

        let mut probes = Vec::new();
        let mut warnings = Vec::new();

        for name in probe_names {
            let baseline_summary = baseline.get(&name);
            let current_summary = current.get(&name);
            let observation =
                compare_probe(&name, baseline_summary, current_summary, &mut warnings);
            probes.push(observation);
        }

        let statuses: Vec<_> = probes.iter().map(|probe| probe.status).collect();
        let mut verdict = aggregate_verdict(&statuses);
        verdict.reasons = probes
            .iter()
            .filter(|probe| !matches!(probe.status, MetricStatus::Pass))
            .flat_map(|probe| probe.reasons.iter().cloned())
            .collect();

        let receipt = ProbeCompareReceipt {
            schema: PROBE_COMPARE_SCHEMA_V1.to_string(),
            tool: req.tool,
            run: make_run_meta(),
            bench: merge_bench(req.baseline.bench, req.current.bench),
            scenario: req.current.scenario.or(req.baseline.scenario),
            baseline_ref: Some(req.baseline_ref),
            current_ref: Some(req.current_ref),
            probes,
            verdict,
            warnings,
        };

        Ok(ProbeCompareOutcome { receipt })
    }
}

fn compare_probe(
    name: &str,
    baseline: Option<&ProbeSummary>,
    current: Option<&ProbeSummary>,
    warnings: &mut Vec<String>,
) -> ProbeCompareObservation {
    let parent = current
        .and_then(|summary| summary.parent.clone())
        .or_else(|| baseline.and_then(|summary| summary.parent.clone()));
    let scope = current
        .and_then(|summary| summary.scope)
        .or_else(|| baseline.and_then(|summary| summary.scope));
    let baseline_count = baseline.map(|summary| summary.count).unwrap_or(0);
    let current_count = current.map(|summary| summary.count).unwrap_or(0);
    let mut deltas = BTreeMap::new();
    let mut reasons = Vec::new();

    let (Some(baseline), Some(current)) = (baseline, current) else {
        let reason = if baseline.is_none() {
            format!("probe '{name}' missing from baseline")
        } else {
            format!("probe '{name}' missing from current")
        };
        warnings.push(reason.clone());
        reasons.push(reason);
        return ProbeCompareObservation {
            name: name.to_string(),
            parent,
            scope,
            baseline_count,
            current_count,
            deltas,
            status: MetricStatus::Warn,
            reasons,
        };
    };

    let metric_names: BTreeSet<String> = baseline
        .metrics
        .keys()
        .chain(current.metrics.keys())
        .cloned()
        .collect();

    for metric in metric_names {
        let baseline_metric = baseline.metrics.get(&metric);
        let current_metric = current.metrics.get(&metric);
        let (Some(baseline_metric), Some(current_metric)) = (baseline_metric, current_metric)
        else {
            let reason = if baseline_metric.is_none() {
                format!("probe '{name}' metric '{metric}' missing from baseline")
            } else {
                format!("probe '{name}' metric '{metric}' missing from current")
            };
            warnings.push(reason.clone());
            reasons.push(reason);
            continue;
        };

        if baseline_metric.unit.as_deref() != current_metric.unit.as_deref() {
            let reason = format!(
                "probe '{name}' metric '{metric}' unit changed from {:?} to {:?}",
                baseline_metric.unit, current_metric.unit
            );
            warnings.push(reason.clone());
            reasons.push(reason);
        }
        if baseline_metric.statistic != current_metric.statistic {
            let reason = format!(
                "probe '{name}' metric '{metric}' statistic changed from {} to {}",
                baseline_metric.statistic.as_str(),
                current_metric.statistic.as_str()
            );
            warnings.push(reason.clone());
            reasons.push(reason);
        }

        let delta = build_delta(
            &metric,
            baseline_metric.value,
            current_metric.value,
            current_metric.statistic,
            &mut reasons,
        );
        if delta.regression > f64::EPSILON {
            reasons.push(format!(
                "probe '{name}' metric '{metric}' regressed by {:.2}%",
                delta.regression * 100.0
            ));
        }
        deltas.insert(metric, delta);
    }

    let status = if reasons.is_empty() {
        MetricStatus::Pass
    } else {
        MetricStatus::Warn
    };

    ProbeCompareObservation {
        name: name.to_string(),
        parent,
        scope,
        baseline_count,
        current_count,
        deltas,
        status,
        reasons,
    }
}
fn build_delta(
    metric: &str,
    baseline: f64,
    current: f64,
    statistic: MetricStatistic,
    reasons: &mut Vec<String>,
) -> Delta {
    let direction = probe_metric_direction(metric);
    let (ratio, pct, regression) = if baseline.abs() <= f64::EPSILON {
        if current.abs() <= f64::EPSILON {
            (1.0, 0.0, 0.0)
        } else {
            match direction {
                Direction::Lower => {
                    reasons.push(format!(
                        "metric '{metric}' has zero baseline and non-zero current"
                    ));
                    (0.0, 1.0, 1.0)
                }
                Direction::Higher => (0.0, 0.0, 0.0),
            }
        }
    } else {
        (
            current / baseline,
            (current - baseline) / baseline,
            calculate_regression(baseline, current, direction),
        )
    };

    Delta {
        baseline,
        current,
        ratio,
        pct,
        regression,
        cv: None,
        noise_threshold: None,
        statistic,
        significance: None,
        status: if regression > f64::EPSILON {
            MetricStatus::Warn
        } else {
            MetricStatus::Pass
        },
    }
}

fn probe_metric_direction(metric: &str) -> Direction {
    if let Some(metric) = Metric::parse_key(metric) {
        return metric.default_direction();
    }

    if metric.ends_with("_per_s")
        || metric.ends_with("_per_sec")
        || metric.contains("throughput")
        || metric.contains("rate")
        || metric == "items"
        || metric.ends_with("_items")
        || metric.ends_with("_count")
    {
        Direction::Higher
    } else {
        Direction::Lower
    }
}

#[derive(Debug, Default)]
struct ProbeSummary {
    count: u32,
    parent: Option<String>,
    scope: Option<ProbeScope>,
    metrics: BTreeMap<String, AggregatedMetric>,
}

#[derive(Debug, Clone)]
struct AggregatedMetric {
    value: f64,
    unit: Option<String>,
    statistic: MetricStatistic,
}

#[derive(Debug, Default)]
struct MetricAccumulator {
    values: Vec<f64>,
    unit: Option<String>,
    statistic: Option<MetricStatistic>,
}

fn summarize_probes(probes: &[ProbeObservation]) -> BTreeMap<String, ProbeSummary> {
    let mut summaries: BTreeMap<String, ProbeSummaryBuilder> = BTreeMap::new();

    for probe in probes {
        let entry = summaries.entry(probe.name.clone()).or_default();
        entry.count += 1;
        if entry.parent.is_none() {
            entry.parent = probe.parent.clone();
        }
        if entry.scope.is_none() {
            entry.scope = probe.scope;
        }

        for (name, metric) in &probe.metrics {
            entry.add_metric(name, metric);
        }
    }

    summaries
        .into_iter()
        .map(|(name, builder)| (name, builder.finish()))
        .collect()
}

#[derive(Debug, Default)]
struct ProbeSummaryBuilder {
    count: u32,
    parent: Option<String>,
    scope: Option<ProbeScope>,
    metrics: BTreeMap<String, MetricAccumulator>,
}

impl ProbeSummaryBuilder {
    fn add_metric(&mut self, name: &str, metric: &ProbeMetricValue) {
        self.add_raw_metric(
            name,
            metric.value,
            metric.unit.as_deref(),
            metric.statistic.as_deref().and_then(parse_metric_statistic),
        );
    }

    fn add_raw_metric(
        &mut self,
        name: &str,
        value: f64,
        unit: Option<&str>,
        statistic: Option<MetricStatistic>,
    ) {
        if !value.is_finite() {
            return;
        }

        let entry = self.metrics.entry(name.to_string()).or_default();
        entry.values.push(value);
        if entry.unit.is_none() {
            entry.unit = unit.map(str::to_string);
        }
        if entry.statistic.is_none() {
            entry.statistic = statistic;
        }
    }

    fn finish(self) -> ProbeSummary {
        let metrics = self
            .metrics
            .into_iter()
            .filter_map(|(name, metric)| {
                median(metric.values).map(|value| {
                    let statistic = metric.statistic.unwrap_or(MetricStatistic::Median);
                    (
                        name,
                        AggregatedMetric {
                            value,
                            unit: metric.unit,
                            statistic,
                        },
                    )
                })
            })
            .collect();

        ProbeSummary {
            count: self.count,
            parent: self.parent,
            scope: self.scope,
            metrics,
        }
    }
}
fn merge_bench(baseline: Option<BenchMeta>, current: Option<BenchMeta>) -> Option<BenchMeta> {
    current.or(baseline)
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

fn parse_metric_statistic(statistic: &str) -> Option<MetricStatistic> {
    match statistic {
        "median" | "p50" => Some(MetricStatistic::Median),
        "p95" => Some(MetricStatistic::P95),
        _ => None,
    }
}

fn make_run_meta() -> RunMeta {
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    RunMeta {
        id: Uuid::new_v4().to_string(),
        started_at: timestamp.clone(),
        ended_at: timestamp,
        host: HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{PROBE_SCHEMA_V1, VerdictStatus};

    fn probe_receipt(probes: Vec<ProbeObservation>) -> ProbeReceipt {
        ProbeReceipt {
            schema: PROBE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate-ingest".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "probe-run".to_string(),
                started_at: "2026-05-08T00:00:00Z".to_string(),
                ended_at: "2026-05-08T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: None,
            scenario: Some("large_file_parse".to_string()),
            probes,
            metadata: BTreeMap::new(),
        }
    }

    fn probe(name: &str, metric: &str, value: f64) -> ProbeObservation {
        probe_with_unit(name, metric, value, Some("ms"))
    }

    fn probe_with_unit(
        name: &str,
        metric: &str,
        value: f64,
        unit: Option<&str>,
    ) -> ProbeObservation {
        ProbeObservation {
            name: name.to_string(),
            parent: Some("parser.total".to_string()),
            scope: Some(ProbeScope::Local),
            iteration: None,
            started_at: None,
            ended_at: None,
            items: None,
            metrics: BTreeMap::from([(
                metric.to_string(),
                ProbeMetricValue {
                    value,
                    unit: unit.map(str::to_string),
                    statistic: None,
                },
            )]),
            attributes: BTreeMap::new(),
        }
    }
    #[test]
    fn probe_compare_matches_by_name_and_computes_deltas() {
        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![probe("parser.tokenize", "wall_ms", 10.0)]),
            current: probe_receipt(vec![probe("parser.tokenize", "wall_ms", 12.0)]),
            baseline_ref: CompareRef {
                path: Some("baselines/probes.json".to_string()),
                run_id: Some("base".to_string()),
            },
            current_ref: CompareRef {
                path: Some("artifacts/probes.json".to_string()),
                run_id: Some("current".to_string()),
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        let receipt = outcome.receipt;
        assert_eq!(receipt.schema, PROBE_COMPARE_SCHEMA_V1);
        assert_eq!(receipt.scenario.as_deref(), Some("large_file_parse"));
        assert_eq!(receipt.probes.len(), 1);
        assert_eq!(receipt.probes[0].name, "parser.tokenize");
        assert_eq!(receipt.probes[0].status, MetricStatus::Warn);
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
        assert!((receipt.probes[0].deltas["wall_ms"].pct - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn probe_compare_treats_per_second_metrics_as_higher_is_better() {
        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![probe("parser.batch_loop", "items_per_s", 100.0)]),
            current: probe_receipt(vec![probe("parser.batch_loop", "items_per_s", 125.0)]),
            baseline_ref: CompareRef {
                path: Some("baselines/probes.json".to_string()),
                run_id: Some("base".to_string()),
            },
            current_ref: CompareRef {
                path: Some("artifacts/probes.json".to_string()),
                run_id: Some("current".to_string()),
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        let receipt = outcome.receipt;
        let delta = &receipt.probes[0].deltas["items_per_s"];
        assert!(delta.pct > 0.0);
        assert_eq!(delta.regression, 0.0);
        assert_eq!(delta.status, MetricStatus::Pass);
        assert_eq!(receipt.probes[0].status, MetricStatus::Pass);
        assert_eq!(receipt.verdict.status, VerdictStatus::Pass);
    }

    #[test]
    fn probe_compare_warns_on_missing_probe() {
        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![probe("parser.tokenize", "wall_ms", 10.0)]),
            current: probe_receipt(vec![probe("parser.ast_build", "wall_ms", 8.0)]),
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        assert_eq!(outcome.receipt.probes.len(), 2);
        assert_eq!(outcome.receipt.verdict.status, VerdictStatus::Warn);
        assert!(
            outcome
                .receipt
                .warnings
                .iter()
                .any(|warning| warning.contains("missing from current"))
        );
    }

    #[test]
    fn probe_compare_uses_median_for_repeated_observations() {
        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![
                probe("parser.tokenize", "wall_ms", 10.0),
                probe("parser.tokenize", "wall_ms", 20.0),
                probe("parser.tokenize", "wall_ms", 100.0),
            ]),
            current: probe_receipt(vec![probe("parser.tokenize", "wall_ms", 12.0)]),
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        let delta = &outcome.receipt.probes[0].deltas["wall_ms"];
        assert_eq!(outcome.receipt.probes[0].baseline_count, 3);
        assert_eq!(delta.baseline, 20.0);
        assert_eq!(delta.current, 12.0);
        assert_eq!(delta.status, MetricStatus::Pass);
        assert_eq!(delta.statistic, MetricStatistic::Median);
    }

    #[test]
    fn probe_compare_warns_on_unit_mismatch() {
        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![probe_with_unit(
                "parser.tokenize",
                "wall_ms",
                10.0,
                Some("ms"),
            )]),
            current: probe_receipt(vec![probe_with_unit(
                "parser.tokenize",
                "wall_ms",
                10.0,
                Some("seconds"),
            )]),
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        assert_eq!(outcome.receipt.probes[0].status, MetricStatus::Warn);
        assert!(
            outcome.receipt.probes[0]
                .reasons
                .iter()
                .any(|reason| reason.contains("unit changed"))
        );
        assert_eq!(outcome.receipt.verdict.status, VerdictStatus::Warn);
    }
    #[test]
    fn probe_compare_treats_items_as_metadata_not_metric() {
        let mut baseline_probe = probe("parser.tokenize", "wall_ms", 10.0);
        baseline_probe.items = Some(10);
        let mut current_probe = probe("parser.tokenize", "wall_ms", 10.0);
        current_probe.items = Some(20);

        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline: probe_receipt(vec![baseline_probe]),
            current: probe_receipt(vec![current_probe]),
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        })
        .expect("compare probes");

        assert!(!outcome.receipt.probes[0].deltas.contains_key("items"));
        assert_eq!(outcome.receipt.probes[0].status, MetricStatus::Pass);
    }
}
