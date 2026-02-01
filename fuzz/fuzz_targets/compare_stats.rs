//! Fuzz target for the compare_stats function in perfgate-domain.
//!
//! This target uses structure-aware fuzzing with the Arbitrary trait to generate
//! valid Stats and Budget inputs, verifying that compare_stats never panics
//! regardless of the input values.
//!
//! **Validates: Requirements 5.4, 5.6**

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;

// Local types that derive Arbitrary for structure-aware fuzzing.
// These mirror the perfgate-types structures but with Arbitrary support.
// Task 7.4 will add Arbitrary derive directly to perfgate-types.

#[derive(Arbitrary, Debug, Clone)]
struct FuzzU64Summary {
    median: u64,
    min: u64,
    max: u64,
}

impl FuzzU64Summary {
    fn to_perfgate(&self) -> perfgate_types::U64Summary {
        // Ensure min <= median <= max invariant
        let mut vals = [self.min, self.median, self.max];
        vals.sort();
        perfgate_types::U64Summary {
            min: vals[0],
            median: vals[1],
            max: vals[2],
        }
    }
}

#[derive(Arbitrary, Debug, Clone)]
struct FuzzF64Summary {
    median: f64,
    min: f64,
    max: f64,
}

impl FuzzF64Summary {
    fn to_perfgate(&self) -> perfgate_types::F64Summary {
        // Filter out NaN values and ensure min <= median <= max invariant
        let filter_nan = |v: f64| if v.is_nan() { 0.0 } else { v };
        let mut vals = [
            filter_nan(self.min),
            filter_nan(self.median),
            filter_nan(self.max),
        ];
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        perfgate_types::F64Summary {
            min: vals[0],
            median: vals[1],
            max: vals[2],
        }
    }
}

#[derive(Arbitrary, Debug, Clone)]
struct FuzzStats {
    wall_ms: FuzzU64Summary,
    has_max_rss_kb: bool,
    max_rss_kb: FuzzU64Summary,
    has_throughput: bool,
    throughput_per_s: FuzzF64Summary,
}

impl FuzzStats {
    fn to_perfgate(&self) -> perfgate_types::Stats {
        perfgate_types::Stats {
            wall_ms: self.wall_ms.to_perfgate(),
            max_rss_kb: if self.has_max_rss_kb {
                Some(self.max_rss_kb.to_perfgate())
            } else {
                None
            },
            throughput_per_s: if self.has_throughput {
                Some(self.throughput_per_s.to_perfgate())
            } else {
                None
            },
        }
    }
}

#[derive(Arbitrary, Debug, Clone, Copy)]
enum FuzzDirection {
    Lower,
    Higher,
}

impl FuzzDirection {
    fn to_perfgate(self) -> perfgate_types::Direction {
        match self {
            FuzzDirection::Lower => perfgate_types::Direction::Lower,
            FuzzDirection::Higher => perfgate_types::Direction::Higher,
        }
    }
}

#[derive(Arbitrary, Debug, Clone, Copy)]
enum FuzzMetric {
    WallMs,
    MaxRssKb,
    ThroughputPerS,
}

impl FuzzMetric {
    fn to_perfgate(self) -> perfgate_types::Metric {
        match self {
            FuzzMetric::WallMs => perfgate_types::Metric::WallMs,
            FuzzMetric::MaxRssKb => perfgate_types::Metric::MaxRssKb,
            FuzzMetric::ThroughputPerS => perfgate_types::Metric::ThroughputPerS,
        }
    }
}

#[derive(Arbitrary, Debug, Clone)]
struct FuzzBudget {
    threshold: f64,
    warn_threshold: f64,
    direction: FuzzDirection,
}

impl FuzzBudget {
    fn to_perfgate(&self) -> perfgate_types::Budget {
        // Filter NaN and ensure thresholds are non-negative
        let filter = |v: f64| {
            if v.is_nan() || v.is_infinite() {
                0.2 // Default to 20% threshold
            } else {
                v.abs()
            }
        };
        let threshold = filter(self.threshold);
        let warn_threshold = filter(self.warn_threshold);

        // Ensure warn_threshold <= threshold
        let (warn_threshold, threshold) = if warn_threshold > threshold {
            (threshold, warn_threshold)
        } else {
            (warn_threshold, threshold)
        };

        perfgate_types::Budget {
            threshold,
            warn_threshold,
            direction: self.direction.to_perfgate(),
        }
    }
}

#[derive(Arbitrary, Debug)]
struct FuzzBudgetEntry {
    metric: FuzzMetric,
    budget: FuzzBudget,
}

#[derive(Arbitrary, Debug)]
struct CompareStatsInput {
    baseline: FuzzStats,
    current: FuzzStats,
    /// Up to 3 budget entries (one per metric type)
    budget_entries: Vec<FuzzBudgetEntry>,
}

fuzz_target!(|input: CompareStatsInput| {
    let baseline = input.baseline.to_perfgate();
    let current = input.current.to_perfgate();

    // Build budgets map, limiting to 3 entries (one per metric)
    let mut budgets: BTreeMap<perfgate_types::Metric, perfgate_types::Budget> = BTreeMap::new();
    for entry in input.budget_entries.iter().take(3) {
        budgets.insert(entry.metric.to_perfgate(), entry.budget.to_perfgate());
    }

    // Call compare_stats - it should never panic regardless of input
    // It may return an error (e.g., InvalidBaseline), which is fine
    let _ = perfgate_domain::compare_stats(&baseline, &current, &budgets);
});
