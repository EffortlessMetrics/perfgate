//! Integration tests for `perfgate export` command
//!
//! **Validates: Requirements for export functionality**

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// Returns the path to the test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Helper function to run compare and generate a compare receipt
fn generate_compare_receipt(
    baseline: &PathBuf,
    current: &PathBuf,
    output_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("perfgate")?;
    cmd.arg("compare")
        .arg("--baseline")
        .arg(baseline)
        .arg("--current")
        .arg(current)
        .arg("--out")
        .arg(output_path);

    // We don't care about the exit code here, just that the receipt is generated
    let _ = cmd.output();
    Ok(())
}

// ============================================================================
// Run receipt export tests
// ============================================================================

/// Test exporting run receipt to CSV format
#[test]
fn test_export_run_to_csv() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(export_path.exists(), "export file should exist");

    // Read and verify content
    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Verify CSV header
    assert!(
        content.starts_with("bench_name,wall_ms_median,wall_ms_min,wall_ms_max,max_rss_kb_median,throughput_median,sample_count,timestamp\n"),
        "CSV should have correct header. Got: {}",
        content.lines().next().unwrap_or("")
    );

    // Verify benchmark name is present
    assert!(
        content.contains("test-benchmark"),
        "CSV should contain benchmark name"
    );

    // Verify wall_ms stats
    assert!(
        content.contains("100,98,102"),
        "CSV should contain wall_ms stats (median=100, min=98, max=102)"
    );

    // Verify max_rss_kb median
    assert!(
        content.contains("1024"),
        "CSV should contain max_rss_kb median"
    );
}

/// Test exporting run receipt to JSONL format
#[test]
fn test_export_run_to_jsonl() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.jsonl");

    let baseline = fixtures_dir().join("baseline.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(export_path.exists(), "export file should exist");

    // Read and verify content
    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Should be a single line of valid JSON
    let lines: Vec<&str> = content.trim().split('\n').collect();
    assert_eq!(
        lines.len(),
        1,
        "JSONL should have exactly 1 line for run receipt"
    );

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("should be valid JSON");
    assert_eq!(parsed["bench_name"], "test-benchmark");
    assert_eq!(parsed["wall_ms_median"], 100);
    assert_eq!(parsed["wall_ms_min"], 98);
    assert_eq!(parsed["wall_ms_max"], 102);
    assert_eq!(parsed["sample_count"], 5);
}

// ============================================================================
// Compare receipt export tests
// ============================================================================

/// Test exporting compare receipt to CSV format
#[test]
fn test_export_compare_to_csv() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // First, generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(export_path.exists(), "export file should exist");

    // Read and verify content
    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Verify CSV header
    assert!(
        content.starts_with(
            "bench_name,metric,baseline_value,current_value,regression_pct,status,threshold\n"
        ),
        "CSV should have correct header"
    );

    // Verify benchmark name and metrics
    assert!(
        content.contains("test-benchmark"),
        "CSV should contain benchmark name"
    );
    assert!(
        content.contains("wall_ms"),
        "CSV should contain wall_ms metric"
    );
}

/// Test exporting compare receipt to JSONL format
#[test]
fn test_export_compare_to_jsonl() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let export_path = temp_dir.path().join("export.jsonl");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // First, generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(export_path.exists(), "export file should exist");

    // Read and verify content
    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Should have one line per metric
    let lines: Vec<&str> = content
        .trim()
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect();
    assert!(
        lines.len() >= 1,
        "JSONL should have at least 1 line per metric"
    );

    // All lines should be valid JSON
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).expect("should be valid JSON");
        assert!(
            parsed.get("bench_name").is_some(),
            "each line should have bench_name"
        );
        assert!(
            parsed.get("metric").is_some(),
            "each line should have metric"
        );
    }
}

// ============================================================================
// Format and argument tests
// ============================================================================

/// Test default format is CSV
#[test]
fn test_export_default_format_is_csv() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    let content = fs::read_to_string(&export_path).expect("failed to read export file");
    assert!(
        content.starts_with("bench_name,"),
        "default format should be CSV with header"
    );
}

/// Test invalid format returns error
#[test]
fn test_export_invalid_format() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("invalid")
        .arg("--out")
        .arg(&export_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid format"));
}

/// Test mutually exclusive --run and --compare
#[test]
fn test_export_run_and_compare_mutually_exclusive() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--compare")
        .arg(&baseline)
        .arg("--out")
        .arg(&export_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// Test missing required argument
#[test]
fn test_export_missing_input() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export").arg("--out").arg(&export_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--run").or(predicate::str::contains("--compare")));
}

/// Test missing input file
#[test]
fn test_export_missing_input_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");
    let nonexistent = temp_dir.path().join("nonexistent.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--run")
        .arg(&nonexistent)
        .arg("--out")
        .arg(&export_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

// ============================================================================
// Stable ordering tests
// ============================================================================

/// Test that CSV output is deterministic across multiple runs
#[test]
fn test_export_csv_deterministic() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let export_path1 = temp_dir.path().join("export1.csv");
    let export_path2 = temp_dir.path().join("export2.csv");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    // Export twice
    for export_path in [&export_path1, &export_path2] {
        let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
        cmd.arg("export")
            .arg("--compare")
            .arg(&compare_receipt_path)
            .arg("--format")
            .arg("csv")
            .arg("--out")
            .arg(export_path);

        cmd.assert().success();
    }

    let content1 = fs::read_to_string(&export_path1).expect("failed to read export1");
    let content2 = fs::read_to_string(&export_path2).expect("failed to read export2");

    assert_eq!(content1, content2, "CSV export should be deterministic");
}

/// Test metrics are sorted alphabetically in compare export
#[test]
fn test_export_compare_metrics_sorted() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let export_path = temp_dir.path().join("export.csv");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    cmd.assert().success();

    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Check that max_rss_kb comes before wall_ms (alphabetical order)
    let max_rss_pos = content.find("max_rss_kb");
    let wall_ms_pos = content.find("wall_ms");

    if let (Some(max_rss), Some(wall_ms)) = (max_rss_pos, wall_ms_pos) {
        assert!(
            max_rss < wall_ms,
            "max_rss_kb should come before wall_ms (alphabetical order)"
        );
    }
}
