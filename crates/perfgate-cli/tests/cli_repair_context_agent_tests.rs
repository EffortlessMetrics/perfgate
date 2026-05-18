//! Agent-facing repair-context fixture coverage.
//!
//! These tests keep the repair context useful as an agent-operable receipt
//! without changing the `perfgate.repair_context.v1` schema.

use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

#[cfg(unix)]
fn slow_command() -> Vec<&'static str> {
    vec!["sh", "-c", "sleep 0.05"]
}

#[cfg(windows)]
fn slow_command() -> Vec<&'static str> {
    vec!["powershell", "-Command", "Start-Sleep -Milliseconds 50"]
}

fn failing_command() -> Vec<&'static str> {
    vec!["perfgate-command-that-does-not-exist-for-agent-fixture"]
}

fn write_config(root: &Path, bench_name: &str, command: &[&str], threshold: f64) {
    let command = command
        .iter()
        .map(|part| format!("\"{}\"", part.replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        root.join("perfgate.toml"),
        format!(
            r#"
[defaults]
repeat = 1
warmup = 0
threshold = {threshold}

[[bench]]
name = "{bench_name}"
command = [{command}]
"#
        ),
    )
    .expect("write config");
}

fn write_baseline(root: &Path, bench_name: &str, wall_ms: u64) {
    let baselines = root.join("baselines");
    fs::create_dir_all(&baselines).expect("create baselines dir");
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.18.0"},
        "run": {
            "id": "baseline-run-id",
            "started_at": "2026-05-18T00:00:00Z",
            "ended_at": "2026-05-18T00:00:01Z",
            "host": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH}
        },
        "bench": {
            "name": bench_name,
            "command": ["echo", "baseline"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": wall_ms, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": {
            "wall_ms": {"median": wall_ms, "min": wall_ms, "max": wall_ms}
        }
    });
    fs::write(
        baselines.join(format!("{bench_name}.json")),
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .expect("write baseline");
}

fn read_repair_context(out_dir: &Path) -> Value {
    let path = out_dir.join("repair_context.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "repair_context.json should exist at {}: {err}",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("repair_context.json should be valid JSON")
}

fn command_contains(repair: &Value, needle: &str) -> bool {
    repair["recommended_next_commands"]
        .as_array()
        .expect("recommended_next_commands array")
        .iter()
        .filter_map(|value| value.as_str())
        .any(|command| command.contains(needle))
}

#[test]
fn missing_baseline_repair_context_is_agent_operable() {
    let temp_dir = tempdir().expect("temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    write_config(
        temp_dir.path(),
        "agent-missing-baseline",
        &success_command(),
        0.20,
    );

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg("perfgate.toml")
        .arg("--bench")
        .arg("agent-missing-baseline")
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("run check");

    assert!(
        output.status.success(),
        "missing baseline is a warning-only setup state: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Status: missing_baseline"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("setup is incomplete") || stderr.contains("Setup is incomplete"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("do not loosen thresholds"),
        "stderr: {stderr}"
    );

    let repair = read_repair_context(&out_dir);
    assert_eq!(repair["schema"], "perfgate.repair_context.v1");
    assert_eq!(repair["benchmark"], "agent-missing-baseline");
    assert_eq!(repair["status"], "warn");
    assert!(repair.get("compare_receipt_path").is_none());
    assert!(
        repair["report_path"]
            .as_str()
            .unwrap()
            .ends_with("report.json")
    );
    assert!(command_contains(&repair, "rerun current command"));
    assert!(command_contains(&repair, "perfgate paired"));
}

#[test]
fn regression_repair_context_preserves_compare_and_breached_metric() {
    let temp_dir = tempdir().expect("temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    write_config(temp_dir.path(), "agent-regression", &slow_command(), 0.01);
    write_baseline(temp_dir.path(), "agent-regression", 1);

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg("perfgate.toml")
        .arg("--bench")
        .arg("agent-regression")
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("run check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "regression should fail policy: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Status: performance_regression"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("do not promote the current run"),
        "stderr: {stderr}"
    );

    let repair = read_repair_context(&out_dir);
    assert_eq!(repair["schema"], "perfgate.repair_context.v1");
    assert_eq!(repair["benchmark"], "agent-regression");
    assert_eq!(repair["status"], "fail");
    assert!(
        repair["compare_receipt_path"]
            .as_str()
            .unwrap()
            .ends_with("compare.json")
    );
    let breached = repair["breached_metrics"]
        .as_array()
        .expect("breached metrics array");
    assert!(
        breached
            .iter()
            .any(|metric| metric["metric"] == "wall_ms" && metric["status"] == "fail"),
        "breached metrics: {breached:?}"
    );
    assert!(command_contains(&repair, "perfgate explain --compare"));
    assert!(command_contains(&repair, "perfgate compare --baseline"));
}

#[test]
fn setup_command_failure_guidance_does_not_invent_repair_context() {
    let temp_dir = tempdir().expect("temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    write_config(
        temp_dir.path(),
        "agent-setup-failure",
        &failing_command(),
        0.20,
    );

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg("perfgate.toml")
        .arg("--bench")
        .arg("agent-setup-failure")
        .arg("--out-dir")
        .arg(&out_dir)
        .output()
        .expect("run check");

    assert!(
        !output.status.success(),
        "command failure should fail setup: stdout {}, stderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Status: setup_command_failed"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("do not loosen thresholds to fix a command that does not run"),
        "stderr: {stderr}"
    );
    assert!(
        !out_dir.join("repair_context.json").exists(),
        "setup failure should not invent a repair context without check receipts"
    );
}
