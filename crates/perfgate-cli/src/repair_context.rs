//! Repair-context artifact construction for failed or warned check runs.

use crate::storage::write_json;
use anyhow::Result;
use perfgate::app::{CheckOutcome, redact_command_for_diagnostics};
use perfgate_types::{
    ChangedFilesSummary, OtelSpanIdentifiers, REPAIR_CONTEXT_SCHEMA_V1, RepairContextReceipt,
    RepairGitMetadata, RepairMetricBreach, VerdictStatus,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command as ProcessCommand;

pub(crate) fn maybe_write_repair_context(
    outcome: &CheckOutcome,
    baseline_path: Option<&Path>,
    emit_requested: bool,
    pretty: bool,
) -> Result<()> {
    let should_emit = emit_requested
        || matches!(
            outcome.report.verdict.status,
            VerdictStatus::Warn | VerdictStatus::Fail
        );
    if !should_emit {
        return Ok(());
    }

    let repair = build_repair_context(outcome, baseline_path);
    let out_path = outcome
        .run_path
        .parent()
        .unwrap_or(Path::new(""))
        .join("repair_context.json");
    write_json(&out_path, &repair, pretty)?;
    Ok(())
}

fn build_repair_context(
    outcome: &CheckOutcome,
    baseline_path: Option<&Path>,
) -> RepairContextReceipt {
    let breached_metrics = if let Some(compare) = &outcome.compare_receipt {
        compare
            .deltas
            .iter()
            .filter_map(|(metric, delta)| {
                if !matches!(delta.status.as_str(), "warn" | "fail" | "skip") {
                    return None;
                }
                let budget = compare.budgets.get(metric)?;
                Some(RepairMetricBreach {
                    metric: *metric,
                    status: delta.status.as_str().to_string(),
                    baseline: delta.baseline,
                    current: delta.current,
                    regression: delta.regression,
                    fail_threshold: budget.threshold,
                    warn_threshold: budget.warn_threshold,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let compare_path = outcome
        .compare_path
        .as_ref()
        .map(|p| p.display().to_string());
    let report_path = outcome.report_path.display().to_string();
    let profile_path = outcome.report.profile_path.clone();
    let otel_span = otel_span_from_env();
    let git = git_metadata();
    let changed_files = changed_files_summary();
    let suggested = recommended_next_commands(outcome, baseline_path);
    let failure_class = repair_failure_class(outcome, &breached_metrics);
    let artifact_paths = repair_artifact_paths(outcome);
    let local_reproduction_command = suggested
        .iter()
        .find(|command| command.starts_with("rerun current command:"))
        .cloned();
    let safe_commands = suggested.clone();
    let forbidden_changes = forbidden_changes(&failure_class);
    let human_review_required = human_review_required(&failure_class);
    let proof_commands_after_repair = proof_commands_after_repair(&outcome.run_receipt.bench.name);

    RepairContextReceipt {
        schema: REPAIR_CONTEXT_SCHEMA_V1.to_string(),
        benchmark: outcome.run_receipt.bench.name.clone(),
        verdict: outcome.report.verdict.clone(),
        status: outcome.report.verdict.status,
        breached_metrics,
        compare_receipt_path: compare_path,
        report_path,
        profile_path,
        git,
        changed_files,
        otel_span,
        recommended_next_commands: suggested,
        failure_class: Some(failure_class),
        artifact_paths,
        local_reproduction_command,
        safe_commands,
        forbidden_changes,
        human_review_required,
        proof_commands_after_repair,
    }
}

fn repair_failure_class(outcome: &CheckOutcome, breaches: &[RepairMetricBreach]) -> String {
    if outcome.compare_receipt.is_none() {
        return "missing_baseline".to_string();
    }
    if outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("host mismatch"))
    {
        return "host_mismatch".to_string();
    }
    if outcome.suggest_paired
        || outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("high noise"))
    {
        return "high_noise".to_string();
    }
    if outcome
        .report
        .verdict
        .reasons
        .iter()
        .any(|reason| reason.contains("review_required") || reason.contains("review required"))
    {
        return "review_required".to_string();
    }
    if !breaches.is_empty()
        || matches!(
            outcome.report.verdict.status,
            VerdictStatus::Warn | VerdictStatus::Fail
        )
    {
        return "performance_regression".to_string();
    }
    "pass".to_string()
}

fn repair_artifact_paths(outcome: &CheckOutcome) -> Vec<String> {
    let mut paths = vec![
        outcome.run_path.display().to_string(),
        outcome.report_path.display().to_string(),
        outcome.markdown_path.display().to_string(),
    ];
    if let Some(compare_path) = &outcome.compare_path {
        paths.push(compare_path.display().to_string());
    }
    if let Some(profile_path) = &outcome.report.profile_path {
        paths.push(profile_path.clone());
    }
    paths.push(
        outcome
            .run_path
            .parent()
            .unwrap_or(Path::new(""))
            .join("repair_context.json")
            .display()
            .to_string(),
    );
    paths.sort();
    paths.dedup();
    paths
}

fn forbidden_changes(failure_class: &str) -> Vec<String> {
    let mut changes = vec![
        "do not loosen fail, warn, or noise thresholds to make this check pass".to_string(),
        "do not make this benchmark required_gate without human review".to_string(),
        "do not require server ledger history for local correctness".to_string(),
    ];
    match failure_class {
        "missing_baseline" => changes
            .push("do not promote the current run as a baseline without human review".to_string()),
        "performance_regression" => changes.push(
            "do not promote the current run as a baseline until the regression is understood"
                .to_string(),
        ),
        "high_noise" => {
            changes.push("do not treat noisy evidence as a confirmed regression".to_string())
        }
        "host_mismatch" => changes.push(
            "do not accept host-mismatched evidence without checking host compatibility"
                .to_string(),
        ),
        "review_required" => {
            changes.push("do not accept a tradeoff without decision evidence".to_string())
        }
        _ => {}
    }
    changes
}

fn human_review_required(failure_class: &str) -> Vec<String> {
    let mut required = vec![
        "threshold changes".to_string(),
        "required_gate policy changes".to_string(),
        "tradeoff acceptance".to_string(),
    ];
    if matches!(
        failure_class,
        "missing_baseline" | "performance_regression" | "host_mismatch"
    ) {
        required.push("baseline promotion".to_string());
    }
    if failure_class == "review_required" {
        required.push("structured decision acceptance".to_string());
    }
    required
}

fn proof_commands_after_repair(bench_name: &str) -> Vec<String> {
    vec![
        format!("perfgate check --config <config> --bench {bench_name} --require-baseline"),
        format!("perfgate review explain --config <config> --bench {bench_name}"),
    ]
}

fn recommended_next_commands(outcome: &CheckOutcome, baseline_path: Option<&Path>) -> Vec<String> {
    let mut cmds = Vec::new();
    let rerun_cmd = redact_command_for_diagnostics(&outcome.run_receipt.bench.command).join(" ");
    if !rerun_cmd.is_empty() {
        cmds.push(format!("rerun current command: {rerun_cmd}"));
    }
    if let Some(compare_path) = &outcome.compare_path {
        cmds.push(format!(
            "perfgate explain --compare {}",
            compare_path.display()
        ));
    }
    cmds.push(format!(
        "perfgate paired --name {} --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat {} --out {}/paired.json",
        outcome.run_receipt.bench.name,
        outcome.run_receipt.bench.repeat.max(10),
        outcome.run_path.parent().unwrap_or(Path::new("")).display()
    ));
    if let Some(base) = baseline_path {
        cmds.push(format!(
            "perfgate compare --baseline {} --current {} --out {}/recompare.json",
            base.display(),
            outcome.run_path.display(),
            outcome.run_path.parent().unwrap_or(Path::new("")).display()
        ));
    }
    cmds.push(
        "perfgate bisect --good <good-ref> --bad HEAD --executable <bench-binary>".to_string(),
    );
    cmds
}

fn otel_span_from_env() -> Option<OtelSpanIdentifiers> {
    let trace_id = std::env::var("OTEL_TRACE_ID").ok();
    let span_id = std::env::var("OTEL_SPAN_ID").ok();
    if trace_id.is_none() && span_id.is_none() {
        None
    } else {
        Some(OtelSpanIdentifiers { trace_id, span_id })
    }
}

pub(crate) fn git_metadata() -> Option<RepairGitMetadata> {
    let branch = run_git_capture(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let sha = run_git_capture(&["rev-parse", "HEAD"]);
    if branch.is_none() && sha.is_none() {
        None
    } else {
        Some(RepairGitMetadata { branch, sha })
    }
}

fn changed_files_summary() -> Option<ChangedFilesSummary> {
    let output = run_git_capture_bytes(&["status", "--porcelain", "-z"])?;
    Some(parse_changed_files_summary(&output))
}

pub(crate) fn parse_changed_files_summary(output: &[u8]) -> ChangedFilesSummary {
    let mut files = Vec::new();
    let mut by_top = BTreeMap::new();

    let mut entries = output
        .split(|byte| *byte == b'\0')
        .filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() <= 3 {
            continue;
        }

        let status = &entry[..2];
        let current_path = if status.iter().any(|code| matches!(code, b'R' | b'C')) {
            entries.next().unwrap_or(&[])
        } else {
            &entry[3..]
        };

        if current_path.is_empty() {
            continue;
        }

        let path = String::from_utf8_lossy(current_path).into_owned();
        files.push(path.clone());
        let top = path
            .split(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(".")
            .to_string();
        *by_top.entry(top).or_insert(0) += 1;
    }

    ChangedFilesSummary {
        file_count: files.len() as u32,
        files,
        file_count_by_top_level: by_top,
    }
}

pub(crate) fn run_git_capture(args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(crate) fn run_git_capture_bytes(args: &[&str]) -> Option<Vec<u8>> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_changed_files_summary_handles_empty_input() {
        let summary = parse_changed_files_summary(b"");
        assert_eq!(summary.file_count, 0);
        assert!(summary.files.is_empty());
        assert!(summary.file_count_by_top_level.is_empty());
    }

    #[test]
    fn parse_changed_files_summary_groups_by_top_level_directory() {
        // Each entry has 2-byte status code, single space, then path, terminated by NUL.
        // Use modified entries (no rename).
        let input = b" M src/lib.rs\0 M src/util.rs\0 M tests/case.rs\0?? README.md\0";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 4);
        assert_eq!(summary.files.len(), 4);
        assert_eq!(summary.file_count_by_top_level["src"], 2);
        assert_eq!(summary.file_count_by_top_level["tests"], 1);
        // top-level file gets "README.md" bucket
        assert_eq!(summary.file_count_by_top_level["README.md"], 1);
    }

    #[test]
    fn parse_changed_files_summary_handles_rename_two_path_entries() {
        // Rename status uses two NUL-separated paths: "<old>\0<new>".
        // The current path is the second entry (the new name).
        let input = b"R  src/old.rs\0src/new.rs\0 M src/touched.rs\0";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 2);
        assert_eq!(summary.files, vec!["src/new.rs", "src/touched.rs"]);
        assert_eq!(summary.file_count_by_top_level["src"], 2);
    }

    #[test]
    fn parse_changed_files_summary_handles_copy_two_path_entries() {
        // Copy status is parsed like rename and should use the destination path.
        let input = b"C  src/template.rs src/copied.rs ";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files, vec!["src/copied.rs"]);
        assert_eq!(summary.file_count_by_top_level["src"], 1);
    }

    #[test]
    fn parse_changed_files_summary_ignores_rename_without_destination_path() {
        // If git output is truncated and destination path is missing, parser should skip it.
        let input = b"R  src/old.rs ";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 0);
        assert!(summary.files.is_empty());
        assert!(summary.file_count_by_top_level.is_empty());
    }
    #[test]
    fn parse_changed_files_summary_skips_short_entries() {
        // Entries with status header only (no path) must be ignored without panicking.
        let input = b"M \0 M src/ok.rs\0";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files, vec!["src/ok.rs"]);
    }

    #[test]
    fn parse_changed_files_summary_falls_back_to_dot_for_paths_starting_with_separator() {
        // A path whose first component splits to empty (e.g., "/abs") should bucket under ".".
        let input = b" M /abs/path.rs\0";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.file_count_by_top_level["."], 1);
    }

    #[test]
    fn parse_changed_files_summary_handles_windows_style_separators() {
        let input = b" M crates\\perfgate\\src\\main.rs\0";
        let summary = parse_changed_files_summary(input);
        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.file_count_by_top_level["crates"], 1);
    }

    #[test]
    fn run_git_capture_returns_none_for_unknown_git_subcommand() {
        // A bogus git subcommand should exit non-zero, so we expect None.
        let result = run_git_capture(&["this-is-not-a-real-git-subcommand"]);
        assert!(result.is_none());
    }

    #[test]
    fn run_git_capture_bytes_returns_none_for_unknown_git_subcommand() {
        let result = run_git_capture_bytes(&["this-is-not-a-real-git-subcommand"]);
        assert!(result.is_none());
    }
}
