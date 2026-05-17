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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn known_artifact_role_recognizes_every_well_known_filename() {
        let expectations = &[
            (RUN_RECEIPT_FILE, "raw measurement receipt"),
            (COMPARE_RECEIPT_FILE, "baseline/current comparison receipt"),
            ("report.json", "machine-readable verdict summary"),
            ("comment.md", "PR-ready human summary"),
            ("repair_context.json", "local reproduction and repair hints"),
            ("decision.md", "human-readable performance decision"),
            (
                "decision.index.json",
                "index of decision evidence artifacts",
            ),
            ("decision-bundle.json", "portable decision evidence bundle"),
            (
                "probe-compare.json",
                "named probe baseline/current comparison",
            ),
            ("scenario.json", "weighted workload scenario receipt"),
            ("tradeoff.json", "tradeoff policy evaluation receipt"),
        ];
        for (name, role) in expectations {
            assert_eq!(
                known_artifact_role(name),
                Some(*role),
                "missing role for {name}"
            );
        }
    }

    #[test]
    fn known_artifact_role_returns_none_for_unknown_filename() {
        assert!(known_artifact_role("random.txt").is_none());
        assert!(known_artifact_role("perfgate.toml").is_none());
        assert!(known_artifact_role("").is_none());
    }

    #[test]
    fn artifact_next_commands_falls_back_to_check_command_when_no_known_files() {
        let cmds = artifact_next_commands(&[]);
        assert_eq!(
            cmds,
            vec!["perfgate check --config perfgate.toml --all".to_string()]
        );
    }

    #[test]
    fn artifact_next_commands_recommends_comment_inspection_when_comment_present() {
        let known = vec![(PathBuf::from("comment.md"), "PR-ready human summary")];
        let cmds = artifact_next_commands(&known);
        assert_eq!(cmds.len(), 1);
        assert!(cmds[0].contains("inspect artifacts/perfgate/comment.md"));
    }

    #[test]
    fn artifact_next_commands_recommends_repair_context_inspection() {
        let known = vec![(
            PathBuf::from("repair_context.json"),
            "local reproduction and repair hints",
        )];
        let cmds = artifact_next_commands(&known);
        assert!(
            cmds.iter()
                .any(|c| c.contains("inspect repair_context.json"))
        );
    }

    #[test]
    fn artifact_next_commands_recommends_decision_bundle_when_index_present() {
        let known = vec![(
            PathBuf::from("decision.index.json"),
            "index of decision evidence artifacts",
        )];
        let cmds = artifact_next_commands(&known);
        assert!(cmds.iter().any(|c| c.contains("perfgate decision bundle")));
    }

    #[test]
    fn artifact_next_commands_recommends_require_baseline_when_compare_present() {
        let known = vec![(
            PathBuf::from(COMPARE_RECEIPT_FILE),
            "baseline/current comparison receipt",
        )];
        let cmds = artifact_next_commands(&known);
        assert!(cmds.iter().any(|c| c.contains("--require-baseline")));
    }

    #[test]
    fn artifact_next_commands_includes_multiple_recommendations_when_artifacts_overlap() {
        let known = vec![
            (PathBuf::from("comment.md"), "PR-ready human summary"),
            (
                PathBuf::from(COMPARE_RECEIPT_FILE),
                "baseline/current comparison receipt",
            ),
        ];
        let cmds = artifact_next_commands(&known);
        assert!(
            cmds.iter()
                .any(|c| c.contains("inspect artifacts/perfgate/comment.md"))
        );
        assert!(cmds.iter().any(|c| c.contains("--require-baseline")));
        assert!(
            !cmds
                .iter()
                .any(|c| c == "perfgate check --config perfgate.toml --all")
        );
    }

    #[test]
    fn artifact_next_commands_compares_by_filename_only() {
        // Even if the path has a subdirectory prefix, file_name() should match.
        let known = vec![(
            PathBuf::from("bench-a/comment.md"),
            "PR-ready human summary",
        )];
        let cmds = artifact_next_commands(&known);
        assert!(
            cmds.iter()
                .any(|c| c.contains("inspect artifacts/perfgate/comment.md"))
        );
    }

    #[test]
    fn collect_artifact_files_returns_empty_when_directory_missing() {
        let tmp = tempdir().unwrap();
        let nonexistent = tmp.path().join("nope");
        let mut known = Vec::new();
        let mut unknown = Vec::new();
        collect_artifact_files(&nonexistent, &nonexistent, &mut known, &mut unknown).unwrap();
        assert!(known.is_empty());
        assert!(unknown.is_empty());
    }

    #[test]
    fn collect_artifact_files_classifies_known_and_unknown_files() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("run.json"), "{}").unwrap();
        fs::write(root.join("report.json"), "{}").unwrap();
        fs::write(root.join("README.txt"), "info").unwrap();

        let mut known = Vec::new();
        let mut unknown = Vec::new();
        collect_artifact_files(root, root, &mut known, &mut unknown).unwrap();
        let known_names: Vec<String> = known
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(known_names.contains(&"run.json".to_string()));
        assert!(known_names.contains(&"report.json".to_string()));
        let unknown_names: Vec<String> = unknown
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(unknown_names, vec!["README.txt"]);
    }

    #[test]
    fn collect_artifact_files_recurses_into_subdirectories() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let nested = root.join("bench-a");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("compare.json"), "{}").unwrap();

        let mut known = Vec::new();
        let mut unknown = Vec::new();
        collect_artifact_files(root, root, &mut known, &mut unknown).unwrap();
        assert_eq!(known.len(), 1);
        let (path, role) = &known[0];
        assert_eq!(path, &PathBuf::from("bench-a/compare.json"));
        assert_eq!(*role, "baseline/current comparison receipt");
    }

    #[test]
    fn collect_artifact_files_sorts_known_and_unknown_lists() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        // Write in an order that's not sorted.
        fs::write(root.join("report.json"), "{}").unwrap();
        fs::write(root.join("compare.json"), "{}").unwrap();
        fs::write(root.join("zzz-unknown.txt"), "x").unwrap();
        fs::write(root.join("aaa-unknown.txt"), "x").unwrap();

        let mut known = Vec::new();
        let mut unknown = Vec::new();
        collect_artifact_files(root, root, &mut known, &mut unknown).unwrap();

        let known_paths: Vec<PathBuf> = known.iter().map(|(p, _)| p.clone()).collect();
        let mut sorted = known_paths.clone();
        sorted.sort();
        assert_eq!(known_paths, sorted);
        let mut sorted_unknown = unknown.clone();
        sorted_unknown.sort();
        assert_eq!(unknown, sorted_unknown);
    }

    #[test]
    fn execute_explain_artifacts_succeeds_when_out_dir_missing() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("perfgate-nope");
        let result = execute_explain_artifacts(ExplainArtifactsArgs { out_dir: missing });
        assert!(result.is_ok());
    }

    #[test]
    fn execute_explain_artifacts_succeeds_for_empty_dir() {
        let tmp = tempdir().unwrap();
        let out_dir = tmp.path().to_path_buf();
        let result = execute_explain_artifacts(ExplainArtifactsArgs { out_dir });
        assert!(result.is_ok());
    }

    #[test]
    fn execute_explain_artifacts_succeeds_with_known_artifacts() {
        let tmp = tempdir().unwrap();
        let out_dir = tmp.path().to_path_buf();
        fs::write(out_dir.join("run.json"), "{}").unwrap();
        fs::write(out_dir.join("comment.md"), "ok").unwrap();
        let result = execute_explain_artifacts(ExplainArtifactsArgs { out_dir });
        assert!(result.is_ok());
    }

    #[test]
    fn execute_explain_action_dispatches_artifacts_variant() {
        let tmp = tempdir().unwrap();
        let out_dir = tmp.path().to_path_buf();
        let result =
            execute_explain_action(ExplainAction::Artifacts(ExplainArtifactsArgs { out_dir }));
        assert!(result.is_ok());
    }
}
