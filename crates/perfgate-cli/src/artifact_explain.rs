//! Artifact explanation command support.

use anyhow::Context;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{COMPARE_RECEIPT_FILE, ExplainAction, ExplainArtifactsArgs, RUN_RECEIPT_FILE};

pub(crate) fn execute_explain_action(action: ExplainAction) -> anyhow::Result<()> {
    match action {
        ExplainAction::Artifacts(args) => execute_explain_artifacts(args),
    }
}

fn execute_explain_artifacts(args: ExplainArtifactsArgs) -> anyhow::Result<()> {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    collect_artifact_files(&args.out_dir, &args.out_dir, &mut known, &mut unknown)?;

    println!("perfgate artifact explanation");
    println!();

    if !args.out_dir.exists() {
        println!("Status: no_artifacts");
        println!(
            "Meaning: {} does not exist yet; run a check or decision command first.",
            args.out_dir.display()
        );
        println!("Artifacts:");
        println!("  none");
        println!("Next:");
        println!("  perfgate check --config perfgate.toml --all");
        println!("Do not:");
        println!("  commit generated artifact directories before reviewing repo policy");
        return Ok(());
    }

    if known.is_empty() {
        println!("Status: no_known_artifacts");
        println!(
            "Meaning: {} exists, but no known perfgate receipt files were found.",
            args.out_dir.display()
        );
        println!("Artifacts:");
        if unknown.is_empty() {
            println!("  none");
        } else {
            for path in &unknown {
                println!("  {}  unrecognized file", path.display());
            }
        }
        println!("Next:");
        println!("  perfgate check --config perfgate.toml --all");
        println!("Do not:");
        println!("  infer a verdict from unknown files alone");
        return Ok(());
    }

    println!("Status: artifacts_found");
    println!("Meaning: known perfgate receipts or review artifacts are present.");
    println!("Artifacts:");
    for (path, role) in &known {
        println!("  {:<32} {}", path.display(), role);
    }
    if !unknown.is_empty() {
        println!("  unknown:");
        for path in &unknown {
            println!("    {}  unrecognized file", path.display());
        }
    }
    println!("Next:");
    for command in artifact_next_commands(&known) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  treat artifacts as durable baselines unless promoted intentionally");

    Ok(())
}

fn collect_artifact_files(
    root: &Path,
    dir: &Path,
    known: &mut Vec<(PathBuf, &'static str)>,
    unknown: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_artifact_files(root, &path, known, unknown)?;
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            unknown.push(relative);
            continue;
        };

        if let Some(role) = known_artifact_role(name) {
            known.push((relative, role));
        } else {
            unknown.push(relative);
        }
    }

    known.sort_by(|left, right| left.0.cmp(&right.0));
    unknown.sort();
    Ok(())
}

fn known_artifact_role(file_name: &str) -> Option<&'static str> {
    match file_name {
        RUN_RECEIPT_FILE => Some("raw measurement receipt"),
        COMPARE_RECEIPT_FILE => Some("baseline/current comparison receipt"),
        "report.json" => Some("machine-readable verdict summary"),
        "comment.md" => Some("PR-ready human summary"),
        "repair_context.json" => Some("local reproduction and repair hints"),
        "decision.md" => Some("human-readable performance decision"),
        "decision.index.json" => Some("index of decision evidence artifacts"),
        "decision-bundle.json" => Some("portable decision evidence bundle"),
        "probe-compare.json" => Some("named probe baseline/current comparison"),
        "scenario.json" => Some("weighted workload scenario receipt"),
        "tradeoff.json" => Some("tradeoff policy evaluation receipt"),
        _ => None,
    }
}

fn artifact_next_commands(known: &[(PathBuf, &'static str)]) -> Vec<String> {
    let has = |name: &str| {
        known
            .iter()
            .any(|(path, _)| path.file_name().and_then(|file| file.to_str()) == Some(name))
    };

    let mut commands = Vec::new();
    if has("comment.md") {
        commands.push("inspect artifacts/perfgate/comment.md or the per-bench comment.md".into());
    }
    if has("repair_context.json") {
        commands.push("inspect repair_context.json for local reproduction and repair hints".into());
    }
    if has("decision.index.json") {
        commands
            .push("perfgate decision bundle --index artifacts/perfgate/decision.index.json".into());
    }
    if has(COMPARE_RECEIPT_FILE) {
        commands.push("perfgate check --config perfgate.toml --all --require-baseline".into());
    }
    if commands.is_empty() {
        commands.push("perfgate check --config perfgate.toml --all".into());
    }
    commands
}
