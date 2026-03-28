use crate::CheckOutcome;
use perfgate_types::{
    REPAIR_CONTEXT_SCHEMA_V1, RepairArtifactRefs, RepairContext, RepairGitContext,
    RepairMetricBreach, VerdictStatus,
};

#[derive(Debug, Clone, Default)]
pub struct RepairContextOptions {
    pub git: Option<RepairGitContext>,
    pub span_ids: Vec<String>,
}

pub fn build_repair_context(
    outcome: &CheckOutcome,
    options: RepairContextOptions,
) -> RepairContext {
    let verdict = outcome.report.verdict.status;
    let reasons = outcome.report.verdict.reasons.clone();
    let benchmark = outcome.run_receipt.bench.name.clone();

    let mut breached_metrics = Vec::new();
    if let Some(compare) = &outcome.compare_receipt {
        for (metric, delta) in &compare.deltas {
            if matches!(
                delta.status,
                perfgate_types::MetricStatus::Warn | perfgate_types::MetricStatus::Fail
            ) && let Some(budget) = compare.budgets.get(metric)
            {
                breached_metrics.push(RepairMetricBreach {
                    metric: *metric,
                    statistic: delta.statistic,
                    status: delta.status,
                    threshold: budget.threshold,
                    warn_threshold: budget.warn_threshold,
                    baseline: delta.baseline,
                    current: delta.current,
                    regression_pct: delta.pct * 100.0,
                });
            }
        }
    }

    let mut recommended_next_commands = vec![
        format!(
            "perfgate blame --baseline {} --current Cargo.lock",
            "old-Cargo.lock"
        ),
        format!(
            "perfgate paired --name {} --baseline-cmd \"<cmd>\" --current-cmd \"<cmd>\" --repeat 10 --out paired.json",
            benchmark
        ),
    ];
    if verdict == VerdictStatus::Fail {
        recommended_next_commands.push(
            "perfgate bisect --good <sha> --bad HEAD --executable <bench-binary>".to_string(),
        );
    }
    if let Some(compare_path) = &outcome.compare_path {
        recommended_next_commands.insert(
            0,
            format!("perfgate explain --compare {}", compare_path.display()),
        );
    }

    RepairContext {
        schema: REPAIR_CONTEXT_SCHEMA_V1.to_string(),
        benchmark,
        verdict,
        reasons,
        breached_metrics,
        artifacts: RepairArtifactRefs {
            compare_receipt_path: outcome
                .compare_path
                .as_ref()
                .map(|p| p.display().to_string()),
            report_path: outcome.report_path.display().to_string(),
            profile_path: outcome.report.profile_path.clone(),
        },
        git: options.git,
        span_ids: options.span_ids,
        recommended_next_commands: recommended_next_commands
            .into_iter()
            .map(|cmd| redact_sensitive_tokens(&cmd))
            .collect(),
    }
}

/// Redact common key=value secret tokens in free-form command text.
pub fn redact_sensitive_tokens(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if (lower.contains("token=")
                || lower.contains("password=")
                || lower.contains("secret=")
                || lower.contains("api_key="))
                && token.contains('=')
            {
                let (k, _) = token.split_once('=').unwrap_or((token, ""));
                format!("{k}=[REDACTED]")
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::redact_sensitive_tokens;

    #[test]
    fn redact_sensitive_tokens_masks_common_secret_patterns() {
        let input = "perfgate check --foo token=abc123 password=s3cr3t api_key=xyz secret=q";
        let output = redact_sensitive_tokens(input);
        assert!(output.contains("token=[REDACTED]"));
        assert!(output.contains("password=[REDACTED]"));
        assert!(output.contains("api_key=[REDACTED]"));
        assert!(output.contains("secret=[REDACTED]"));
        assert!(!output.contains("abc123"));
    }
}
