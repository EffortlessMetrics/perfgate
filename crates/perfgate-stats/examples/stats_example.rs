//! Basic example demonstrating statistical functions.
//!
//! Run with: cargo run -p perfgate-stats --example basic

use perfgate_stats::{mean_and_variance, percentile, summarize_f64, summarize_u64};

fn main() {
    println!("=== perfgate-stats Basic Example ===\n");

    println!("1. Summarizing u64 benchmark data:");
    let u64_samples: Vec<u64> = vec![95, 100, 105, 98, 102, 100, 103];
    let u64_summary = summarize_u64(&u64_samples).expect("should have samples");
    println!("   Samples: {:?}", u64_samples);
    println!("   Min: {} ms", u64_summary.min);
    println!("   Median: {} ms", u64_summary.median);
    println!("   Max: {} ms", u64_summary.max);

    println!("\n2. Summarizing f64 benchmark data:");
    let f64_samples: Vec<f64> = vec![1.5, 2.3, 1.8, 2.1, 1.9, 2.0, 1.7];
    let f64_summary = summarize_f64(&f64_samples).expect("should have samples");
    println!("   Samples: {:?}", f64_samples);
    println!("   Min: {:.2}", f64_summary.min);
    println!("   Median: {:.2}", f64_summary.median);
    println!("   Max: {:.2}", f64_summary.max);

    println!("\n3. Computing percentiles:");
    let latency_samples: Vec<f64> =
        vec![10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 100.0];
    println!("   Samples: {:?}", latency_samples);

    let p50 = percentile(latency_samples.clone(), 0.50).expect("should have samples");
    println!("   P50 (median): {:.1}", p50);

    let p90 = percentile(latency_samples.clone(), 0.90).expect("should have samples");
    println!("   P90: {:.1}", p90);

    let p95 = percentile(latency_samples.clone(), 0.95).expect("should have samples");
    println!("   P95: {:.1}", p95);

    let p99 = percentile(latency_samples.clone(), 0.99).expect("should have samples");
    println!("   P99: {:.1}", p99);

    println!("\n4. Computing mean and variance:");
    let samples: Vec<f64> = vec![10.0, 12.0, 14.0, 16.0, 18.0];
    if let Some((mean, variance)) = mean_and_variance(&samples) {
        println!("   Samples: {:?}", samples);
        println!("   Mean: {:.2}", mean);
        println!("   Variance: {:.2}", variance);
        println!("   Std Dev: {:.2}", variance.sqrt());
    }

    println!("\n5. Edge case - single element:");
    let single = vec![42u64];
    let single_summary = summarize_u64(&single).expect("should work with single element");
    println!("   Single sample: {}", single[0]);
    println!("   Min = Median = Max = {}", single_summary.median);

    println!("\n6. Even vs odd count median calculation:");
    let odd_count: Vec<u64> = vec![1, 2, 3, 4, 5];
    let odd_summary = summarize_u64(&odd_count).unwrap();
    println!(
        "   Odd count {:?}: median = {}",
        odd_count, odd_summary.median
    );

    let even_count: Vec<u64> = vec![1, 2, 3, 4, 5, 6];
    let even_summary = summarize_u64(&even_count).unwrap();
    println!(
        "   Even count {:?}: median = {}",
        even_count, even_summary.median
    );

    println!("\n=== Example complete ===");
}
