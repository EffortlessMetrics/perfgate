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
    }
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
