//! Check-command diagnostic guidance and remediation hints.
//!
//! This module owns user-facing failure classification, artifact discovery,
//! and next-step command rendering so the check execution path can focus on
//! orchestration.

use std::path::Path;

use perfgate::app::CheckOutcome;
use perfgate_types::error::{AdapterError, ConfigValidationError, IoError, PerfgateError};

use super::{COMPARE_RECEIPT_FILE, CheckConfig, RUN_RECEIPT_FILE, is_regression};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailureClass {
    SetupMissingConfig,
    SetupMissingBench,
    SetupCommandFailed,
    MissingBaseline,
    PerformanceRegression,
    HighNoise,
    UnsupportedMetric,
    HostMismatch,
    ReviewRequired,
    ServerUploadFailed,
}

impl FailureClass {
    fn status(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => "setup_missing_config",
            Self::SetupMissingBench => "setup_missing_bench",
            Self::SetupCommandFailed => "setup_command_failed",
            Self::MissingBaseline => "missing_baseline",
            Self::PerformanceRegression => "performance_regression",
            Self::HighNoise => "high_noise",
            Self::UnsupportedMetric => "unsupported_metric",
            Self::HostMismatch => "host_mismatch",
            Self::ReviewRequired => "review_required",
            Self::ServerUploadFailed => "server_upload_failed",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => {
                "perfgate could not read the config, so no performance decision was made."
            }
            Self::SetupMissingBench => {
                "The requested benchmark is missing or no runnable benchmarks are configured yet."
            }
            Self::SetupCommandFailed => {
                "The benchmark command failed before perfgate could make a performance decision."
            }
            Self::MissingBaseline => "Setup is incomplete; this is not a performance regression.",
            Self::PerformanceRegression => {
                "A configured benchmark exceeded its performance budget or warning threshold."
            }
            Self::HighNoise => {
                "The run is noisy enough that the result may need paired mode or calibration."
            }
            Self::UnsupportedMetric => "A requested metric is not available on this platform.",
            Self::HostMismatch => {
                "The baseline and current run were captured on different host fingerprints."
            }
            Self::ReviewRequired => {
                "The evidence needs human review before this result should be accepted."
            }
            Self::ServerUploadFailed => {
                "The optional server ledger upload failed; local receipts still record the result."
            }
        }
    }

    fn do_not(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => "do not copy another repo's baselines to bypass setup",
            Self::SetupMissingBench => {
                "do not promote a baseline until the benchmark command is reviewed"
            }
            Self::SetupCommandFailed => {
                "do not loosen thresholds to fix a command that does not run"
            }
            Self::MissingBaseline => "do not loosen thresholds to fix a missing baseline",
            Self::PerformanceRegression => {
                "do not promote the current run as a baseline until the regression is understood"
            }
            Self::HighNoise => "do not treat noisy single-run evidence as release proof",
            Self::UnsupportedMetric => {
                "do not assume a missing platform metric invalidates every gate"
            }
            Self::HostMismatch => {
                "do not accept host-mismatched evidence without checking whether the hosts are comparable"
            }
            Self::ReviewRequired => "do not bypass required review by changing local thresholds",
            Self::ServerUploadFailed => {
                "do not rerun the benchmark just to repair an optional ledger upload"
            }
        }
    }

    fn artifacts(self, out_dir: Option<&Path>, compare_path: Option<&Path>) -> Vec<String> {
        let Some(out_dir) = out_dir else {
            return vec![
                "artifacts unavailable because setup failed before receipts were written"
                    .to_string(),
            ];
        };

        let mut artifacts = match self {
            Self::MissingBaseline => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
                out_dir.join(COMMENT_MARKDOWN_FILE).display().to_string(),
                out_dir.join("repair_context.json").display().to_string(),
            ],
            Self::PerformanceRegression
            | Self::HighNoise
            | Self::HostMismatch
            | Self::ReviewRequired => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                compare_path
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| out_dir.join(COMPARE_RECEIPT_FILE).display().to_string()),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
                out_dir.join(COMMENT_MARKDOWN_FILE).display().to_string(),
                out_dir.join("repair_context.json").display().to_string(),
            ],
            Self::ServerUploadFailed => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
            ],
            Self::SetupMissingConfig
            | Self::SetupMissingBench
            | Self::SetupCommandFailed
            | Self::UnsupportedMetric => {
                vec!["artifacts unavailable or incomplete because setup failed".to_string()]
            }
        };
        artifacts.sort();
        artifacts.dedup();
        artifacts
    }

    fn next_commands(
        self,
        config_path: &Path,
        bench_name: Option<&str>,
        compare_path: Option<&Path>,
    ) -> Vec<String> {
        let config = shell_path(config_path);
        let check = check_command(config_path, bench_name, false);
        let check_required = check_command(config_path, bench_name, true);
        match self {
            Self::SetupMissingConfig => vec![
                "perfgate init --ci github --profile standard --suggest-benches".to_string(),
                format!("perfgate doctor --config {config}"),
            ],
            Self::SetupMissingBench => vec![
                format!("edit {config} and add a reviewed [[bench]] command"),
                format!("perfgate doctor --config {config}"),
            ],
            Self::SetupCommandFailed => vec![check],
            Self::MissingBaseline => vec![check, baseline_promote_command(config_path, bench_name)],
            Self::PerformanceRegression => {
                let mut commands = vec![check_required];
                if let Some(compare_path) = compare_path {
                    commands.push(format!(
                        "perfgate explain --compare {}",
                        shell_path(compare_path)
                    ));
                }
                commands
            }
            Self::HighNoise => vec![
                paired_command(bench_name),
                format!("review noise guidance before tightening {config}"),
            ],
            Self::UnsupportedMetric => vec![
                format!("review platform metric support before changing {config}"),
                check,
            ],
            Self::HostMismatch => vec![
                check_required,
                "rerun on the same runner class as the baseline".to_string(),
            ],
            Self::ReviewRequired => vec![
                "review artifacts/perfgate/decision.md or the Action summary".to_string(),
                "perfgate decision bundle --index artifacts/perfgate/decision.index.json"
                    .to_string(),
            ],
            Self::ServerUploadFailed => vec![
                "inspect server URL/API key/project settings".to_string(),
                "perfgate decision history".to_string(),
            ],
        }
    }
}

const REPORT_RECEIPT_FILE: &str = "report.json";
const COMMENT_MARKDOWN_FILE: &str = "comment.md";

pub(super) fn shell_path(path: &Path) -> String {
    let value = path.display().to_string();
    if value.contains(' ') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value
    }
}

pub(super) fn check_command(
    config_path: &Path,
    bench_name: Option<&str>,
    require_baseline: bool,
) -> String {
    let mut command = if let Some(bench_name) = bench_name {
        format!(
            "perfgate check --config {} --bench {}",
            shell_path(config_path),
            bench_name
        )
    } else {
        format!("perfgate check --config {} --all", shell_path(config_path))
    };
    if require_baseline {
        command.push_str(" --require-baseline");
    }
    command
}

fn baseline_promote_command(config_path: &Path, bench_name: Option<&str>) -> String {
    if let Some(bench_name) = bench_name {
        format!(
            "perfgate baseline promote --config {} --bench {}",
            shell_path(config_path),
            bench_name
        )
    } else {
        format!(
            "perfgate baseline promote --config {} --all",
            shell_path(config_path)
        )
    }
}

pub(super) fn paired_command(bench_name: Option<&str>) -> String {
    let name = bench_name.unwrap_or("<bench>");
    format!(
        "perfgate paired --name {name} --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat 10 --out artifacts/perfgate/{name}/paired.json"
    )
}

pub(super) fn print_check_failure_guidance(
    class: FailureClass,
    config_path: &Path,
    bench_name: Option<&str>,
    out_dir: Option<&Path>,
    compare_path: Option<&Path>,
) {
    eprintln!();
    eprintln!("Status: {}", class.status());
    eprintln!("Meaning: {}", class.meaning());
    eprintln!("Artifacts:");
    for artifact in class.artifacts(out_dir, compare_path) {
        eprintln!("  {artifact}");
    }
    eprintln!("Next:");
    for command in class.next_commands(config_path, bench_name, compare_path) {
        eprintln!("  {command}");
    }
    eprintln!("Do not:");
    eprintln!("  {}", class.do_not());
}

pub(super) fn classify_check_error(error: &anyhow::Error) -> FailureClass {
    if let Some(err) = error.downcast_ref::<PerfgateError>() {
        return match err {
            PerfgateError::Config(ConfigValidationError::BenchName(_)) => {
                FailureClass::SetupMissingBench
            }
            PerfgateError::Io(IoError::BaselineNotFound { .. }) => FailureClass::MissingBaseline,
            PerfgateError::Io(IoError::RunCommand { .. })
            | PerfgateError::Adapter(AdapterError::RunCommand { .. })
            | PerfgateError::Adapter(AdapterError::EmptyArgv)
            | PerfgateError::Adapter(AdapterError::Timeout) => FailureClass::SetupCommandFailed,
            PerfgateError::Adapter(AdapterError::TimeoutUnsupported) => {
                FailureClass::UnsupportedMetric
            }
            _ => FailureClass::SetupCommandFailed,
        };
    }

    let message = error.to_string().to_ascii_lowercase();
    if message.contains("no benchmarks")
        || message.contains("not found in config")
        || message.contains("either --bench or --all")
    {
        FailureClass::SetupMissingBench
    } else if message.contains("baseline") {
        FailureClass::MissingBaseline
    } else if message.contains("host mismatch") {
        FailureClass::HostMismatch
    } else if message.contains("not found") || message.contains("read ") {
        FailureClass::SetupMissingConfig
    } else {
        FailureClass::SetupCommandFailed
    }
}

pub(super) fn emit_check_outcome_guidance(
    req: &CheckConfig,
    bench_name: &str,
    bench_out_dir: &Path,
    outcome: &CheckOutcome,
) {
    if outcome.compare_receipt.is_none() {
        print_check_failure_guidance(
            FailureClass::MissingBaseline,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            None,
        );
    }

    if let Some(compare) = &outcome.compare_receipt
        && is_regression(compare.verdict.status)
    {
        print_check_failure_guidance(
            FailureClass::PerformanceRegression,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome.suggest_paired
        || outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("high noise"))
    {
        print_check_failure_guidance(
            FailureClass::HighNoise,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("host mismatch"))
    {
        print_check_failure_guidance(
            FailureClass::HostMismatch,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome
        .report
        .verdict
        .reasons
        .iter()
        .any(|reason| reason.contains("review_required") || reason.contains("review required"))
    {
        print_check_failure_guidance(
            FailureClass::ReviewRequired,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn slash_paths(values: &[String]) -> Vec<String> {
        values
            .iter()
            .map(|value| value.replace('\\', "/"))
            .collect()
    }

    #[test]
    fn shell_path_returns_plain_value_for_simple_paths() {
        let p = Path::new("perfgate.toml");
        assert_eq!(shell_path(p), "perfgate.toml");
    }

    #[test]
    fn shell_path_quotes_paths_with_spaces() {
        let p = PathBuf::from("/tmp/has space/file.toml");
        let quoted = shell_path(&p);
        assert!(
            quoted.starts_with('"') && quoted.ends_with('"'),
            "got {quoted}"
        );
        assert!(quoted.contains("has space"), "got {quoted}");
    }

    #[test]
    fn shell_path_escapes_embedded_quotes_when_quoting() {
        let p = PathBuf::from("with \"quote\" and space");
        let quoted = shell_path(&p);
        assert_eq!(quoted, "\"with \\\"quote\\\" and space\"");
    }

    #[test]
    fn check_command_uses_bench_when_provided() {
        let cfg = Path::new("perfgate.toml");
        assert_eq!(
            check_command(cfg, Some("bench-a"), false),
            "perfgate check --config perfgate.toml --bench bench-a"
        );
    }

    #[test]
    fn check_command_uses_all_flag_when_no_bench() {
        let cfg = Path::new("perfgate.toml");
        assert_eq!(
            check_command(cfg, None, false),
            "perfgate check --config perfgate.toml --all"
        );
    }

    #[test]
    fn check_command_appends_require_baseline_when_requested() {
        let cfg = Path::new("perfgate.toml");
        let with = check_command(cfg, Some("bench-a"), true);
        assert!(with.ends_with("--require-baseline"), "got: {with}");
        let none = check_command(cfg, None, true);
        assert!(none.ends_with("--require-baseline"), "got: {none}");
    }

    #[test]
    fn check_command_quotes_config_path_with_spaces() {
        let cfg = PathBuf::from("/tmp/has space/perfgate.toml");
        let cmd = check_command(&cfg, Some("b"), false);
        assert!(
            cmd.contains("--config \"/tmp/has space/perfgate.toml\""),
            "got: {cmd}"
        );
    }

    #[test]
    fn paired_command_substitutes_bench_name_in_all_positions() {
        let cmd = paired_command(Some("my-bench"));
        assert!(cmd.contains("--name my-bench"));
        assert!(cmd.contains("artifacts/perfgate/my-bench/paired.json"));
    }

    #[test]
    fn paired_command_uses_placeholder_when_bench_missing() {
        let cmd = paired_command(None);
        assert!(cmd.contains("--name <bench>"));
        assert!(cmd.contains("artifacts/perfgate/<bench>/paired.json"));
    }

    #[test]
    fn failure_class_status_strings_are_stable() {
        assert_eq!(
            FailureClass::SetupMissingConfig.status(),
            "setup_missing_config"
        );
        assert_eq!(
            FailureClass::SetupMissingBench.status(),
            "setup_missing_bench"
        );
        assert_eq!(
            FailureClass::SetupCommandFailed.status(),
            "setup_command_failed"
        );
        assert_eq!(FailureClass::MissingBaseline.status(), "missing_baseline");
        assert_eq!(
            FailureClass::PerformanceRegression.status(),
            "performance_regression"
        );
        assert_eq!(FailureClass::HighNoise.status(), "high_noise");
        assert_eq!(
            FailureClass::UnsupportedMetric.status(),
            "unsupported_metric"
        );
        assert_eq!(FailureClass::HostMismatch.status(), "host_mismatch");
        assert_eq!(FailureClass::ReviewRequired.status(), "review_required");
        assert_eq!(
            FailureClass::ServerUploadFailed.status(),
            "server_upload_failed"
        );
    }

    #[test]
    fn failure_class_meaning_and_do_not_are_non_empty_for_every_variant() {
        for class in [
            FailureClass::SetupMissingConfig,
            FailureClass::SetupMissingBench,
            FailureClass::SetupCommandFailed,
            FailureClass::MissingBaseline,
            FailureClass::PerformanceRegression,
            FailureClass::HighNoise,
            FailureClass::UnsupportedMetric,
            FailureClass::HostMismatch,
            FailureClass::ReviewRequired,
            FailureClass::ServerUploadFailed,
        ] {
            assert!(!class.meaning().is_empty(), "{:?} has empty meaning", class);
            assert!(!class.do_not().is_empty(), "{:?} has empty do_not", class);
        }
    }

    #[test]
    fn failure_class_artifacts_handles_missing_out_dir() {
        let arts = FailureClass::MissingBaseline.artifacts(None, None);
        assert_eq!(arts.len(), 1);
        assert!(arts[0].contains("artifacts unavailable"), "got: {:?}", arts);
    }

    #[test]
    fn failure_class_artifacts_missing_baseline_lists_expected_files() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        let arts = FailureClass::MissingBaseline.artifacts(Some(&out_dir), None);
        let arts = slash_paths(&arts);
        let joined = arts.join(",");
        assert!(
            joined.contains("artifacts/perfgate/run.json"),
            "got: {joined}"
        );
        assert!(
            joined.contains("artifacts/perfgate/report.json"),
            "got: {joined}"
        );
        assert!(
            joined.contains("artifacts/perfgate/comment.md"),
            "got: {joined}"
        );
        assert!(
            joined.contains("artifacts/perfgate/repair_context.json"),
            "got: {joined}"
        );
    }

    #[test]
    fn failure_class_artifacts_regression_prefers_explicit_compare_path() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        let compare = PathBuf::from("artifacts/perfgate/other/compare.json");
        let arts = FailureClass::PerformanceRegression.artifacts(Some(&out_dir), Some(&compare));
        let arts = slash_paths(&arts);
        assert!(
            arts.iter()
                .any(|a| a == "artifacts/perfgate/other/compare.json"),
            "got: {:?}",
            arts
        );
        assert_eq!(
            arts.iter().filter(|a| a.ends_with("compare.json")).count(),
            1,
            "got: {:?}",
            arts
        );
    }

    #[test]
    fn failure_class_artifacts_regression_falls_back_to_default_compare() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        let arts = FailureClass::PerformanceRegression.artifacts(Some(&out_dir), None);
        let arts = slash_paths(&arts);
        assert!(
            arts.iter().any(|a| a == "artifacts/perfgate/compare.json"),
            "got: {:?}",
            arts
        );
    }

    #[test]
    fn failure_class_artifacts_server_upload_failed_excludes_compare() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        let arts = FailureClass::ServerUploadFailed.artifacts(Some(&out_dir), None);
        let arts = slash_paths(&arts);
        assert!(
            !arts.iter().any(|a| a.contains("compare.json")),
            "got: {:?}",
            arts
        );
        assert!(arts.iter().any(|a| a == "artifacts/perfgate/run.json"));
    }

    #[test]
    fn failure_class_artifacts_setup_variants_say_unavailable() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        for class in [
            FailureClass::SetupMissingConfig,
            FailureClass::SetupMissingBench,
            FailureClass::SetupCommandFailed,
            FailureClass::UnsupportedMetric,
        ] {
            let arts = class.artifacts(Some(&out_dir), None);
            assert_eq!(arts.len(), 1, "{:?}", class);
            assert!(
                arts[0].contains("unavailable") || arts[0].contains("incomplete"),
                "{:?} => {:?}",
                class,
                arts
            );
        }
    }

    #[test]
    fn failure_class_artifacts_sorts_and_dedups() {
        let out_dir = PathBuf::from("artifacts/perfgate");
        let arts = FailureClass::HostMismatch.artifacts(Some(&out_dir), None);
        let mut sorted = arts.clone();
        sorted.sort();
        assert_eq!(arts, sorted, "artifacts must be sorted");
        let mut deduped = arts.clone();
        deduped.dedup();
        assert_eq!(arts, deduped, "artifacts must be deduped");
    }

    #[test]
    fn next_commands_setup_missing_config_recommends_init_and_doctor() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::SetupMissingConfig.next_commands(&cfg, None, None);
        assert!(
            cmds.iter().any(|c| c.contains("perfgate init")),
            "got: {:?}",
            cmds
        );
        assert!(
            cmds.iter().any(|c| c.contains("perfgate doctor")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_missing_baseline_includes_baseline_promote() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::MissingBaseline.next_commands(&cfg, Some("bench-a"), None);
        assert!(
            cmds.iter().any(|c| c.contains("baseline promote")),
            "got: {:?}",
            cmds
        );
        assert!(
            cmds.iter().any(|c| c.contains("--bench bench-a")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_missing_baseline_without_bench_uses_all_flag() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::MissingBaseline.next_commands(&cfg, None, None);
        assert!(
            cmds.iter()
                .any(|c| c.contains("baseline promote") && c.contains("--all")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_performance_regression_appends_explain_when_compare_path_present() {
        let cfg = PathBuf::from("perfgate.toml");
        let compare = PathBuf::from("artifacts/compare.json");
        let cmds =
            FailureClass::PerformanceRegression.next_commands(&cfg, Some("b"), Some(&compare));
        assert!(
            cmds.iter().any(|c| c.contains("--require-baseline")),
            "got: {:?}",
            cmds
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("perfgate explain --compare")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_performance_regression_skips_explain_when_no_compare_path() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::PerformanceRegression.next_commands(&cfg, Some("b"), None);
        assert!(
            !cmds.iter().any(|c| c.contains("perfgate explain")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_high_noise_recommends_paired_run() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::HighNoise.next_commands(&cfg, Some("b"), None);
        assert!(
            cmds.iter().any(|c| c.starts_with("perfgate paired")),
            "got: {:?}",
            cmds
        );
    }

    #[test]
    fn next_commands_host_mismatch_requires_baseline_and_mentions_runner() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::HostMismatch.next_commands(&cfg, Some("b"), None);
        assert!(
            cmds.iter().any(|c| c.contains("--require-baseline")),
            "got: {:?}",
            cmds
        );
        assert!(cmds.iter().any(|c| c.contains("runner")), "got: {:?}", cmds);
    }

    #[test]
    fn next_commands_review_required_points_at_decision_artifacts() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::ReviewRequired.next_commands(&cfg, None, None);
        assert!(
            cmds.iter()
                .any(|c| c.contains("decision.md") || c.contains("Action summary"))
        );
        assert!(cmds.iter().any(|c| c.contains("perfgate decision bundle")));
    }

    #[test]
    fn next_commands_server_upload_failed_recommends_inspection_and_history() {
        let cfg = PathBuf::from("perfgate.toml");
        let cmds = FailureClass::ServerUploadFailed.next_commands(&cfg, None, None);
        assert!(
            cmds.iter()
                .any(|c| c.contains("API key") || c.contains("server URL"))
        );
        assert!(cmds.iter().any(|c| c == "perfgate decision history"));
    }

    #[test]
    fn classify_check_error_recognizes_missing_bench_message() {
        let err = anyhow::anyhow!("benchmark not found in config");
        assert_eq!(classify_check_error(&err), FailureClass::SetupMissingBench);
        let err = anyhow::anyhow!("either --bench or --all must be provided");
        assert_eq!(classify_check_error(&err), FailureClass::SetupMissingBench);
        let err = anyhow::anyhow!("no benchmarks were configured");
        assert_eq!(classify_check_error(&err), FailureClass::SetupMissingBench);
    }

    #[test]
    fn classify_check_error_recognizes_baseline_message() {
        let err = anyhow::anyhow!("baseline could not be loaded");
        assert_eq!(classify_check_error(&err), FailureClass::MissingBaseline);
    }

    #[test]
    fn classify_check_error_recognizes_host_mismatch_message() {
        let err = anyhow::anyhow!("host mismatch detected");
        assert_eq!(classify_check_error(&err), FailureClass::HostMismatch);
    }

    #[test]
    fn classify_check_error_recognizes_config_read_failure() {
        let err = anyhow::anyhow!("read perfgate.toml failed");
        assert_eq!(classify_check_error(&err), FailureClass::SetupMissingConfig);
    }

    #[test]
    fn classify_check_error_default_to_command_failure() {
        let err = anyhow::anyhow!("something weird happened");
        assert_eq!(classify_check_error(&err), FailureClass::SetupCommandFailed);
    }

    #[test]
    fn classify_check_error_handles_perfgate_error_variants() {
        use perfgate_types::error::{AdapterError, ConfigValidationError, IoError, PerfgateError};

        let err: anyhow::Error =
            PerfgateError::Config(ConfigValidationError::BenchName("bad".into())).into();
        assert_eq!(classify_check_error(&err), FailureClass::SetupMissingBench);

        let err: anyhow::Error =
            PerfgateError::Io(IoError::BaselineNotFound { path: "x".into() }).into();
        assert_eq!(classify_check_error(&err), FailureClass::MissingBaseline);

        let err: anyhow::Error = PerfgateError::Adapter(AdapterError::EmptyArgv).into();
        assert_eq!(classify_check_error(&err), FailureClass::SetupCommandFailed);

        let err: anyhow::Error = PerfgateError::Adapter(AdapterError::Timeout).into();
        assert_eq!(classify_check_error(&err), FailureClass::SetupCommandFailed);

        let err: anyhow::Error = PerfgateError::Adapter(AdapterError::TimeoutUnsupported).into();
        assert_eq!(classify_check_error(&err), FailureClass::UnsupportedMetric);
    }
}
