use crate::CheckOutcome;
use perfgate_types::{
    MetricStatus, REPAIR_CONTEXT_SCHEMA_V1, RepairBreachedMetric, RepairChangedFilesSummary,
    RepairContext, RepairGitContext, RepairSpanIdentifiers,
};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct RepairContextOptions {
    pub compare_receipt_path: Option<String>,
    pub report_path: Option<String>,
    pub profile_path: Option<String>,
    pub changed_files: RepairChangedFilesSummary,
    pub git: RepairGitContext,
    pub spans: Option<RepairSpanIdentifiers>,
    pub next_commands: Vec<String>,
}

pub fn build_repair_context(
    outcome: &CheckOutcome,
    options: RepairContextOptions,
) -> RepairContext {
    let breached_metrics = outcome
        .compare_receipt
        .as_ref()
        .map(|compare| {
            compare
                .deltas
                .iter()
                .filter_map(|(metric, delta)| {
                    if matches!(delta.status, MetricStatus::Warn | MetricStatus::Fail) {
                        let budget = compare.budgets.get(metric)?;
                        Some(RepairBreachedMetric {
                            metric: *metric,
                            status: delta.status,
                            threshold: budget.threshold,
                            warn_threshold: budget.warn_threshold,
                            baseline: delta.baseline,
                            current: delta.current,
                            regression_pct: delta.pct * 100.0,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    RepairContext {
        schema: REPAIR_CONTEXT_SCHEMA_V1.to_string(),
        benchmark: outcome.run_receipt.bench.name.clone(),
        verdict: outcome.report.verdict.clone(),
        breached_metrics,
        compare_receipt_path: options.compare_receipt_path,
        report_path: options.report_path,
        profile_path: options.profile_path,
        changed_files: options.changed_files,
        git: options.git,
        spans: options.spans,
        next_commands: options
            .next_commands
            .into_iter()
            .map(|cmd| redact_sensitive(&cmd))
            .collect(),
    }
}

pub fn summarize_changed_files(files: &[String]) -> RepairChangedFilesSummary {
    let mut count_by_area: BTreeMap<String, u32> = BTreeMap::new();
    for file in files {
        let area = classify_area(file);
        *count_by_area.entry(area).or_insert(0) += 1;
    }

    RepairChangedFilesSummary {
        files: files.iter().map(|f| redact_sensitive(f)).collect(),
        total_count: files.len() as u32,
        count_by_area,
    }
}

fn classify_area(path: &str) -> String {
    let p = Path::new(path);
    let mut parts = p.components();

    let Some(first) = parts.next() else {
        return "root".to_string();
    };
    let first = first.as_os_str().to_string_lossy();
    if first == "crates"
        && let Some(second) = parts.next()
    {
        return format!("crates/{}", second.as_os_str().to_string_lossy());
    }
    first.to_string()
}

pub fn redact_sensitive(input: &str) -> String {
    const SENSITIVE_KEYS: &[&str] = &["token", "secret", "password", "apikey", "api_key", "key"];
    let mut out = input.to_string();
    for key in SENSITIVE_KEYS {
        let needle = format!("{key}=");
        if let Some(pos) = out.to_ascii_lowercase().find(&needle) {
            let start = pos + needle.len();
            let end = out[start..]
                .find([' ', '&', ';'])
                .map(|idx| start + idx)
                .unwrap_or(out.len());
            out.replace_range(start..end, "***");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{redact_sensitive, summarize_changed_files};

    #[test]
    fn redacts_sensitive_key_value_pairs() {
        let redacted = redact_sensitive("curl https://x?token=abc123&mode=fast");
        assert!(redacted.contains("token=***"));
        assert!(!redacted.contains("abc123"));
    }

    #[test]
    fn summarize_groups_by_crate() {
        let summary = summarize_changed_files(&[
            "crates/perfgate-cli/src/main.rs".to_string(),
            "crates/perfgate-app/src/check.rs".to_string(),
            "README.md".to_string(),
        ]);
        assert_eq!(summary.total_count, 3);
        assert_eq!(summary.count_by_area.get("crates/perfgate-cli"), Some(&1));
        assert_eq!(summary.count_by_area.get("crates/perfgate-app"), Some(&1));
        assert_eq!(summary.count_by_area.get("README.md"), Some(&1));
    }
}
