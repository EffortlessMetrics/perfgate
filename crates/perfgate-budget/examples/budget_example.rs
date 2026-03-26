//! Demonstrates evaluate_budget and related budget functions.

use perfgate_budget::{aggregate_verdict, calculate_regression, determine_status, evaluate_budget};
use perfgate_types::{Budget, Direction, MetricStatus};

fn main() {
    let budget = Budget {
        noise_threshold: None,
        noise_policy: perfgate_types::NoisePolicy::Ignore,
        threshold: 0.20,      // 20% regression = fail
        warn_threshold: 0.10, // 10% regression = warn
        direction: Direction::Lower,
    };

    // Scenario 1: 5% regression → Pass
    let result = evaluate_budget(100.0, 105.0, &budget, None).expect("evaluate");
    println!(
        "5% regression:  status={:?}, regression={:.1}%",
        result.status,
        result.regression * 100.0
    );
    assert_eq!(result.status, MetricStatus::Pass);

    // Scenario 2: 15% regression → Warn
    let result = evaluate_budget(100.0, 115.0, &budget, None).expect("evaluate");
    println!(
        "15% regression: status={:?}, regression={:.1}%",
        result.status,
        result.regression * 100.0
    );
    assert_eq!(result.status, MetricStatus::Warn);

    // Scenario 3: 25% regression → Fail
    let result = evaluate_budget(100.0, 125.0, &budget, None).expect("evaluate");
    println!(
        "25% regression: status={:?}, regression={:.1}%",
        result.status,
        result.regression * 100.0
    );
    assert_eq!(result.status, MetricStatus::Fail);

    // calculate_regression for Direction::Higher (e.g. throughput)
    let reg = calculate_regression(100.0, 80.0, Direction::Higher);
    println!(
        "\nHigher-is-better: 100→80 regression = {:.1}%",
        reg * 100.0
    );

    // determine_status directly
    let status = determine_status(0.15, 0.20, 0.10);
    println!("determine_status(0.15, 0.20, 0.10) = {:?}", status);

    // aggregate_verdict
    let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn, MetricStatus::Fail]);
    println!(
        "\naggregate_verdict: {:?} (pass={}, warn={}, fail={})",
        verdict.status, verdict.counts.pass, verdict.counts.warn, verdict.counts.fail
    );
}
