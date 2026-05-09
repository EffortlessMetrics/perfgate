use crate::{
    BenchMeta, CompareRef, Delta, MetricStatus, RunMeta, ToolInfo, TradeoffDowngrade, TradeoffRule,
    Verdict,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Scope of a named performance probe inside a workload.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum ProbeScope {
    Local,
    Enclosing,
    Dominant,
    Total,
}

/// A numeric metric observed for a named probe.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ProbeMetricValue {
    pub value: f64,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub unit: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub statistic: Option<String>,
}

/// One named probe observation from external instrumentation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ProbeObservation {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<ProbeScope>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub iteration: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub started_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ended_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub items: Option<u64>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metrics: BTreeMap<String, ProbeMetricValue>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, String>,
}

/// A versioned receipt for named probe observations (`perfgate.probe.v1`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ProbeReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bench: Option<BenchMeta>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scenario: Option<String>,

    pub probes: Vec<ProbeObservation>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Comparison evidence for one named probe.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ProbeCompareObservation {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<ProbeScope>,

    pub baseline_count: u32,
    pub current_count: u32,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deltas: BTreeMap<String, Delta>,

    pub status: MetricStatus,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// A versioned receipt for named probe deltas (`perfgate.probe_compare.v1`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ProbeCompareReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bench: Option<BenchMeta>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scenario: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_ref: Option<CompareRef>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub current_ref: Option<CompareRef>,

    pub probes: Vec<ProbeCompareObservation>,
    pub verdict: Verdict,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Scenario definition captured in a scenario evaluation receipt.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ScenarioMeta {
    pub name: String,
    pub weight: f64,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub command: Option<Vec<String>>,
}

/// A scenario component such as one benchmark, phase, or probe group.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ScenarioComponent {
    pub name: String,
    pub weight: f64,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub benchmark: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub compare_ref: Option<CompareRef>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub probe_compare_ref: Option<CompareRef>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deltas: BTreeMap<String, Delta>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<String>,

    pub status: MetricStatus,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// A versioned receipt for weighted workload scenarios (`perfgate.scenario.v1`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ScenarioReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,
    pub scenario: ScenarioMeta,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_ref: Option<CompareRef>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub current_ref: Option<CompareRef>,

    pub components: Vec<ScenarioComponent>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub weighted_deltas: BTreeMap<String, Delta>,

    pub verdict: Verdict,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Outcome of a tradeoff policy decision.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum TradeoffDecisionStatus {
    Accepted,
    Rejected,
    NeedsReview,
    NotEvaluated,
}

/// Evaluation result for one tradeoff requirement.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffRequirementOutcome {
    pub metric: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub probe: Option<String>,

    pub required_change: f64,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub observed_change: Option<f64>,

    pub satisfied: bool,
    pub status: MetricStatus,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

/// Evaluation result for one local regression allowance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffAllowanceOutcome {
    pub metric: String,
    pub probe: String,
    pub max_regression: f64,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub observed_regression: Option<f64>,

    pub satisfied: bool,
    pub status: MetricStatus,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

/// Evaluation result for one named tradeoff rule.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffRuleOutcome {
    pub name: String,
    pub status: TradeoffDecisionStatus,
    pub accepted: bool,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub downgrade_to: Option<TradeoffDowngrade>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<TradeoffRequirementOutcome>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowances: Vec<TradeoffAllowanceOutcome>,
}

/// Probe-level tradeoff evidence.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffProbeOutcome {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<ProbeScope>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub weight: Option<f64>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub deltas: BTreeMap<String, Delta>,

    pub status: MetricStatus,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

/// Final structured tradeoff decision.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffDecision {
    pub accepted_tradeoff: bool,
    #[serde(default)]
    pub review_required: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_reasons: Vec<String>,

    pub status: MetricStatus,
    pub reason: String,
}

/// A versioned receipt explaining accepted or rejected tradeoffs (`perfgate.tradeoff.v1`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scenario: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_ref: Option<CompareRef>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub current_ref: Option<CompareRef>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configured_rules: Vec<TradeoffRule>,

    pub rules: Vec<TradeoffRuleOutcome>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<TradeoffProbeOutcome>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub weighted_deltas: BTreeMap<String, Delta>,

    pub decision: TradeoffDecision,
    pub verdict: Verdict,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PROBE_COMPARE_SCHEMA_V1, PROBE_SCHEMA_V1, SCENARIO_SCHEMA_V1, TRADEOFF_SCHEMA_V1,
        U64Summary, VerdictCounts, VerdictStatus,
    };

    fn tool() -> ToolInfo {
        ToolInfo {
            name: "perfgate".into(),
            version: "0.16.0".into(),
        }
    }

    fn run() -> RunMeta {
        RunMeta {
            id: "run-1".into(),
            started_at: "2026-05-08T00:00:00Z".into(),
            ended_at: "2026-05-08T00:00:01Z".into(),
            host: crate::HostInfo {
                os: "linux".into(),
                arch: "x86_64".into(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        }
    }

    fn verdict() -> Verdict {
        Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
                skip: 0,
            },
            reasons: Vec::new(),
        }
    }

    fn wall_delta() -> Delta {
        Delta {
            baseline: 100.0,
            current: 92.0,
            ratio: 0.92,
            pct: -0.08,
            regression: 0.0,
            cv: None,
            noise_threshold: None,
            statistic: crate::MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Pass,
        }
    }

    #[test]
    fn probe_receipt_round_trips() {
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "wall_ms".into(),
            ProbeMetricValue {
                value: 12.4,
                unit: Some("ms".into()),
                statistic: None,
            },
        );

        let receipt = ProbeReceipt {
            schema: PROBE_SCHEMA_V1.into(),
            tool: tool(),
            run: run(),
            bench: None,
            scenario: Some("large_file_parse".into()),
            probes: vec![ProbeObservation {
                name: "parser.tokenize".into(),
                parent: Some("request.total".into()),
                scope: Some(ProbeScope::Local),
                iteration: Some(1),
                started_at: None,
                ended_at: None,
                items: Some(10_000),
                metrics,
                attributes: BTreeMap::new(),
            }],
            metadata: BTreeMap::new(),
        };

        let json = serde_json::to_string(&receipt).expect("serialize probe receipt");
        let parsed: ProbeReceipt = serde_json::from_str(&json).expect("parse probe receipt");
        assert_eq!(parsed.schema, PROBE_SCHEMA_V1);
        assert_eq!(parsed.probes[0].name, "parser.tokenize");
    }

    #[test]
    fn scenario_receipt_round_trips() {
        let mut weighted_deltas = BTreeMap::new();
        weighted_deltas.insert("wall_ms".into(), wall_delta());

        let receipt = ScenarioReceipt {
            schema: SCENARIO_SCHEMA_V1.into(),
            tool: tool(),
            run: run(),
            scenario: ScenarioMeta {
                name: "large_file_parse".into(),
                weight: 0.4,
                description: None,
                command: Some(vec!["cargo".into(), "bench".into()]),
            },
            baseline_ref: None,
            current_ref: None,
            components: vec![ScenarioComponent {
                name: "parser.batch_loop".into(),
                weight: 1.0,
                benchmark: Some("large-file".into()),
                compare_ref: None,
                probe_compare_ref: Some(CompareRef {
                    path: Some("artifacts/perfgate/large-file/probe-compare.json".into()),
                    run_id: Some("probe-current".into()),
                }),
                deltas: weighted_deltas.clone(),
                probes: vec!["parser.tokenize".into()],
                status: MetricStatus::Pass,
                reasons: Vec::new(),
            }],
            weighted_deltas,
            verdict: verdict(),
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&receipt).expect("serialize scenario receipt");
        let parsed: ScenarioReceipt = serde_json::from_str(&json).expect("parse scenario receipt");
        assert_eq!(parsed.schema, SCENARIO_SCHEMA_V1);
        assert_eq!(parsed.scenario.name, "large_file_parse");
    }

    #[test]
    fn probe_compare_receipt_round_trips() {
        let receipt = ProbeCompareReceipt {
            schema: PROBE_COMPARE_SCHEMA_V1.into(),
            tool: tool(),
            run: run(),
            bench: Some(crate::BenchMeta {
                name: "parser".into(),
                cwd: None,
                command: vec!["cargo".into(), "bench".into()],
                repeat: 2,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            }),
            scenario: Some("large_file_parse".into()),
            baseline_ref: Some(CompareRef {
                path: Some("baselines/probes.json".into()),
                run_id: Some("baseline-run".into()),
            }),
            current_ref: Some(CompareRef {
                path: Some("artifacts/perfgate/probes.json".into()),
                run_id: Some("current-run".into()),
            }),
            probes: vec![ProbeCompareObservation {
                name: "parser.tokenize".into(),
                parent: Some("parser.total".into()),
                scope: Some(ProbeScope::Local),
                baseline_count: 1,
                current_count: 1,
                deltas: BTreeMap::from([("wall_ms".into(), wall_delta())]),
                status: MetricStatus::Pass,
                reasons: Vec::new(),
            }],
            verdict: verdict(),
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&receipt).expect("serialize probe compare receipt");
        let parsed: ProbeCompareReceipt =
            serde_json::from_str(&json).expect("parse probe compare receipt");
        assert_eq!(parsed.schema, PROBE_COMPARE_SCHEMA_V1);
        assert_eq!(parsed.probes[0].name, "parser.tokenize");
    }

    #[test]
    fn tradeoff_receipt_round_trips() {
        let receipt = TradeoffReceipt {
            schema: TRADEOFF_SCHEMA_V1.into(),
            tool: tool(),
            run: run(),
            scenario: Some("large_file_parse".into()),
            baseline_ref: None,
            current_ref: None,
            configured_rules: Vec::new(),
            rules: vec![TradeoffRuleOutcome {
                name: "tokenizer-slower-if-parser-faster".into(),
                status: TradeoffDecisionStatus::Accepted,
                accepted: true,
                downgrade_to: Some(TradeoffDowngrade::Pass),
                reason: Some("dominant parser loop improved".into()),
                requirements: vec![TradeoffRequirementOutcome {
                    metric: "wall_ms".into(),
                    probe: Some("parser.batch_loop".into()),
                    required_change: -0.08,
                    observed_change: Some(-0.104),
                    satisfied: true,
                    status: MetricStatus::Pass,
                    reason: None,
                }],
                allowances: vec![TradeoffAllowanceOutcome {
                    metric: "wall_ms".into(),
                    probe: "parser.tokenize".into(),
                    max_regression: 0.03,
                    observed_regression: Some(0.021),
                    satisfied: true,
                    status: MetricStatus::Pass,
                    reason: None,
                }],
            }],
            probes: vec![TradeoffProbeOutcome {
                name: "parser.tokenize".into(),
                scope: Some(ProbeScope::Local),
                weight: Some(0.2),
                deltas: BTreeMap::from([("wall_ms".into(), wall_delta())]),
                status: MetricStatus::Warn,
                reason: Some("local slowdown".into()),
            }],
            weighted_deltas: BTreeMap::from([("wall_ms".into(), wall_delta())]),
            decision: TradeoffDecision {
                accepted_tradeoff: true,
                review_required: false,
                review_reasons: Vec::new(),
                status: MetricStatus::Pass,
                reason: "local slowdown offset by dominant-loop improvement".into(),
            },
            verdict: verdict(),
            warnings: Vec::new(),
        };

        let json = serde_json::to_string(&receipt).expect("serialize tradeoff receipt");
        let parsed: TradeoffReceipt = serde_json::from_str(&json).expect("parse tradeoff receipt");
        assert_eq!(parsed.schema, TRADEOFF_SCHEMA_V1);
        assert!(parsed.decision.accepted_tradeoff);
    }

    #[test]
    fn minimal_probe_metric_can_represent_existing_summaries() {
        let wall = U64Summary::new(12, 10, 14);
        let value = ProbeMetricValue {
            value: wall.median as f64,
            unit: Some("ms".into()),
            statistic: Some("median".into()),
        };
        assert_eq!(value.value, 12.0);
    }
}
