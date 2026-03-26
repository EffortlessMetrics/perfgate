//! Demonstrates Welch's t-test via compute_significance.

use perfgate_significance::compute_significance;

fn main() {
    // Two clearly different distributions (significant regression)
    let baseline = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 101.0, 99.0];
    let current = vec![110.0, 112.0, 108.0, 111.0, 109.0, 110.0, 111.0, 109.0];

    let alpha = 0.05;
    let min_samples = 8;

    let result = compute_significance(&baseline, &current, alpha, min_samples);
    match result {
        Some(sig) => {
            println!("Welch's t-test results:");
            println!("  p-value:    {:.6}", sig.p_value.unwrap_or(1.0));
            println!("  alpha:      {}", sig.alpha);
            println!("  significant: {}", sig.significant);
            println!("  baseline_n: {}", sig.baseline_samples);
            println!("  current_n:  {}", sig.current_samples);
            assert!(sig.significant, "Expected significant difference");
        }
        None => println!("Insufficient samples for significance test"),
    }

    // Identical distributions (not significant)
    let same_a = vec![100.0, 101.0, 99.0, 100.0, 102.0, 98.0, 101.0, 99.0];
    let same_b = vec![100.0, 101.0, 99.0, 100.0, 102.0, 98.0, 101.0, 99.0];

    let result2 = compute_significance(&same_a, &same_b, alpha, min_samples);
    match result2 {
        Some(sig) => {
            println!("\nIdentical distributions:");
            println!("  p-value:     {:.6}", sig.p_value.unwrap_or(1.0));
            println!("  significant: {}", sig.significant);
            assert!(!sig.significant, "Expected no significant difference");
        }
        None => println!("\nInsufficient samples"),
    }
}
