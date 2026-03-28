use std::fs;
use std::path::Path;

use serde_json::Value;
use tempfile::tempdir;

mod common;
use common::{fixtures_dir, perfgate_cmd};

fn write_receipt_variant(
    source: &Path,
    destination: &Path,
    run_id: &str,
    os: &str,
    arch: &str,
    failing_sample: bool,
) {
    let content = fs::read_to_string(source).expect("fixture should exist");
    let mut receipt: Value = serde_json::from_str(&content).expect("fixture should be valid JSON");
    receipt["run"]["id"] = Value::String(run_id.to_string());
    receipt["run"]["host"]["os"] = Value::String(os.to_string());
    receipt["run"]["host"]["arch"] = Value::String(arch.to_string());
    if failing_sample {
        receipt["samples"][0]["exit_code"] = Value::from(1);
    }
    fs::write(
        destination,
        serde_json::to_string_pretty(&receipt).expect("receipt should serialize"),
    )
    .expect("variant receipt should be written");
}

#[test]
fn aggregate_emits_formal_aggregate_receipt() {
    let temp_dir = tempdir().unwrap();
    let first = temp_dir.path().join("linux.json");
    let second = temp_dir.path().join("macos.json");
    let out = temp_dir.path().join("aggregate.json");
    let pass_fixture = fixtures_dir().join("current_pass.json");

    write_receipt_variant(
        &pass_fixture,
        &first,
        "agg-pass-1",
        "linux",
        "x86_64",
        false,
    );
    write_receipt_variant(
        &pass_fixture,
        &second,
        "agg-pass-2",
        "macos",
        "aarch64",
        false,
    );

    perfgate_cmd()
        .arg("aggregate")
        .arg(&first)
        .arg(&second)
        .args(["--policy", "weighted"])
        .args(["--quorum", "0.6"])
        .args(["--weight", "linux-x86_64=0.4"])
        .args(["--weight", "macos-aarch64=0.6"])
        .args(["--runner-class", "fleet"])
        .args(["--lane", "nightly"])
        .arg("--out")
        .arg(&out)
        .assert()
        .success();

    let content = fs::read_to_string(&out).expect("aggregate receipt should exist");
    let receipt: Value = serde_json::from_str(&content).expect("aggregate receipt should be JSON");

    assert_eq!(receipt["schema"].as_str(), Some("perfgate.aggregate.v1"));
    assert_eq!(receipt["policy"].as_str(), Some("weighted"));
    assert_eq!(receipt["verdict"]["status"].as_str(), Some("pass"));
    assert_eq!(receipt["weights"]["linux-x86_64"].as_f64(), Some(0.4));
    assert_eq!(receipt["weights"]["macos-aarch64"].as_f64(), Some(0.6));
    assert_eq!(receipt["inputs"].as_array().map(Vec::len), Some(2));
    assert!(
        receipt.get("samples").is_none(),
        "aggregate output must not be a run receipt"
    );

    let inputs = receipt["inputs"]
        .as_array()
        .expect("inputs should be an array");
    assert!(inputs.iter().any(|input| {
        input["runner"]["label"].as_str() == Some("linux-x86_64")
            && input["runner"]["class"].as_str() == Some("fleet")
            && input["runner"]["lane"].as_str() == Some("nightly")
    }));
    assert!(inputs.iter().any(|input| {
        input["runner"]["label"].as_str() == Some("macos-aarch64")
            && input["runner"]["class"].as_str() == Some("fleet")
            && input["runner"]["lane"].as_str() == Some("nightly")
    }));
}

#[test]
fn aggregate_returns_exit_code_2_and_still_writes_receipt_on_policy_failure() {
    let temp_dir = tempdir().unwrap();
    let first = temp_dir.path().join("pass.json");
    let second = temp_dir.path().join("fail.json");
    let out = temp_dir.path().join("aggregate.json");
    let pass_fixture = fixtures_dir().join("current_pass.json");

    write_receipt_variant(
        &pass_fixture,
        &first,
        "agg-pass-1",
        "linux",
        "x86_64",
        false,
    );
    write_receipt_variant(
        &pass_fixture,
        &second,
        "agg-fail-1",
        "linux",
        "x86_64",
        true,
    );

    perfgate_cmd()
        .arg("aggregate")
        .arg(&first)
        .arg(&second)
        .args(["--policy", "all"])
        .arg("--out")
        .arg(&out)
        .assert()
        .code(2);

    let content = fs::read_to_string(&out).expect("aggregate receipt should still be written");
    let receipt: Value = serde_json::from_str(&content).expect("aggregate receipt should be JSON");

    assert_eq!(receipt["schema"].as_str(), Some("perfgate.aggregate.v1"));
    assert_eq!(receipt["verdict"]["status"].as_str(), Some("fail"));
    assert_eq!(receipt["verdict"]["failed"].as_u64(), Some(1));
    assert!(
        receipt["verdict"]["reasons"]
            .as_array()
            .expect("reasons should be an array")
            .iter()
            .any(|reason| reason.as_str() == Some("1 runner(s) failed under all-must-pass policy"))
    );
}
