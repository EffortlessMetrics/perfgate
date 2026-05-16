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
