#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: (Vec<f64>, Vec<f64>)| {
    use perfgate_significance::compute_significance;

    let (baseline, current) = data;
    if baseline.len() >= 2 && current.len() >= 2 {
        let _ = compute_significance(&baseline, &current, 0.05, 2);
    }
});








