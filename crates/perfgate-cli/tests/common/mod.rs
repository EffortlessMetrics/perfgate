#![allow(dead_code)]

use assert_cmd::Command;
use std::env;
use std::path::{Path, PathBuf};

/// Build a perfgate command with coverage passthrough when available.
pub fn perfgate_cmd() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    if let Ok(profile) = env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd
}

/// Returns the path to integration-test fixtures.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Helper to generate a compare receipt fixture.
pub fn generate_compare_receipt(
    baseline: &Path,
    current: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(baseline)
        .arg("--current")
        .arg(current)
        .arg("--out")
        .arg(output_path);

    // The helper only needs the output artifact; verdict exit code is irrelevant here.
    let _ = cmd.output();
    Ok(())
}
