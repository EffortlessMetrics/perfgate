//! Doctor and calibration command implementation.
//!
//! This module keeps setup-health diagnostics and benchmark-threshold calibration
//! separate from argument parsing and command dispatch in `main.rs`.

use crate::{
    COMPARE_RECEIPT_FILE, CalibrateArgs, DoctorArgs, RUN_RECEIPT_FILE, ServerFlags,
    SignalDoctorArgs, check_command, load_optional_baseline_receipt, paired_command, read_json,
    resolve_configured_out_dir, run_git_capture, with_tokio_runtime,
};
use chrono::{DateTime, Utc};
use perfgate::app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate::app::init::{CiPlatform, ci_workflow_path};
use perfgate_client::{BaselineClient, ClientConfig, RetryConfig};
use perfgate_types::config::load_config_file;
use perfgate_types::error::ConfigValidationError;
use perfgate_types::{CompareReceipt, ConfigFile, Metric, RunReceipt};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SIGNAL_MATURE_SAMPLE_LIMIT: usize = 7;
const SIGNAL_HIGH_NOISE_CV: f64 = 0.10;
const SIGNAL_STALE_BASELINE_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug)]
pub(crate) struct DoctorCheck {
    pub(crate) status: DoctorStatus,
    pub(crate) name: &'static str,
    pub(crate) detail: String,
}

impl DoctorCheck {
    pub(crate) fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Ok,
            name,
            detail: detail.into(),
        }
    }

    pub(crate) fn warn(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Warn,
            name,
            detail: detail.into(),
        }
    }

    pub(crate) fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Fail,
            name,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdoptionState {
    NoConfig,
    ConfiguredNoBenches,
    BenchesNoBaselines,
    ReadyLocal,
    ReadyCi,
    DecisionCandidate,
    LedgerConfigured,
}

impl AdoptionState {
    fn status(self) -> &'static str {
        match self {
            Self::NoConfig => "no_config",
            Self::ConfiguredNoBenches => "configured_no_benches",
            Self::BenchesNoBaselines => "benches_no_baselines",
            Self::ReadyLocal => "ready_local",
            Self::ReadyCi => "ready_ci",
            Self::DecisionCandidate => "decision_candidate",
            Self::LedgerConfigured => "ledger_configured",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::NoConfig => "No perfgate config was found for this repo.",
            Self::ConfiguredNoBenches => {
                "Config exists, but no runnable benchmarks are configured yet."
            }
            Self::BenchesNoBaselines => {
                "Benchmarks are configured, but setup is incomplete because baselines are missing."
            }
            Self::ReadyLocal => {
                "Local config and baselines are ready for a required-baseline check."
            }
            Self::ReadyCi => {
                "Local baselines and the generated GitHub Action workflow are present."
            }
            Self::DecisionCandidate => {
                "Structured decision config is present; use it when reviewers need tradeoff evidence."
            }
            Self::LedgerConfigured => {
                "Server ledger settings are configured; local receipts remain the correctness contract."
            }
        }
    }

    fn next(self, config_path: &Path) -> Vec<String> {
        let config = config_path.display();
        match self {
            Self::NoConfig => vec!["perfgate init --ci github --profile standard".to_string()],
            Self::ConfiguredNoBenches => vec![
                format!("edit {config} and add a reviewed [[bench]] command"),
                format!("perfgate doctor --config {config}"),
            ],
            Self::BenchesNoBaselines => vec![
                format!("perfgate check --config {config} --all"),
                format!("perfgate baseline promote --config {config} --all"),
            ],
            Self::ReadyLocal => vec![format!(
                "perfgate check --config {config} --all --require-baseline"
            )],
            Self::ReadyCi => vec![format!(
                "perfgate check --config {config} --all --require-baseline"
            )],
            Self::DecisionCandidate => vec![
                format!("perfgate decision evaluate --config {config}"),
                "perfgate decision bundle --index artifacts/perfgate/decision.index.json"
                    .to_string(),
            ],
            Self::LedgerConfigured => vec![
                "perfgate decision history".to_string(),
                format!("perfgate check --config {config} --all --require-baseline"),
            ],
        }
    }

    fn do_not(self) -> &'static str {
        match self {
            Self::NoConfig => "do not copy another repo's baselines before initializing this repo",
            Self::ConfiguredNoBenches => {
                "do not promote a baseline until the benchmark command measures the workload you care about"
            }
            Self::BenchesNoBaselines => "do not loosen thresholds to fix missing baseline setup",
            Self::ReadyLocal => "do not enable required CI before committing reviewed baselines",
            Self::ReadyCi => "do not debug CI before trying the local reproduction command",
            Self::DecisionCandidate => {
                "do not make structured decisions mandatory for simple local gates"
            }
            Self::LedgerConfigured => {
                "do not treat server ledger upload as local correctness unless policy makes it blocking"
            }
        }
    }
}

fn print_adoption_state(state: AdoptionState, config_path: &Path) {
    println!();
    println!("State: {}", state.status());
    println!("Meaning: {}", state.meaning());
    println!("Next:");
    for command in state.next(config_path) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  {}", state.do_not());
}

fn classify_adoption_state(
    config: Option<&ConfigFile>,
    config_path: &Path,
    server_flags: &ServerFlags,
) -> AdoptionState {
    let Some(config) = config else {
        return AdoptionState::NoConfig;
    };

    if config.benches.is_empty() {
        return AdoptionState::ConfiguredNoBenches;
    }

    if !local_baselines_ready(config, server_flags) {
        return AdoptionState::BenchesNoBaselines;
    }

    if server_flags
        .resolve(&config.baseline_server)
        .is_configured()
    {
        return AdoptionState::LedgerConfigured;
    }

    if !config.scenarios.is_empty() || !config.tradeoffs.is_empty() {
        return AdoptionState::DecisionCandidate;
    }

    let project_root = doctor_project_root(config_path);
    if project_root
        .join(ci_workflow_path(CiPlatform::GitHub))
        .exists()
    {
        return AdoptionState::ReadyCi;
    }

    AdoptionState::ReadyLocal
}

fn doctor_project_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn local_baselines_ready(config: &ConfigFile, server_flags: &ServerFlags) -> bool {
    if config.benches.is_empty() {
        return false;
    }

    if server_flags
        .resolve(&config.baseline_server)
        .is_configured()
    {
        return true;
    }

    let mut local = 0usize;
    let mut found = 0usize;
    let mut remote = 0usize;
    for bench in &config.benches {
        let path = resolve_baseline_path(&None, &bench.name, config);
        let path_text = path.to_string_lossy();
        if is_remote_storage_uri(&path_text) {
            remote += 1;
            continue;
        }
        local += 1;
        if path.exists() {
            found += 1;
        }
    }

    (local == 0 && remote > 0) || (local > 0 && found == local)
}

#[derive(Debug, Clone, Copy)]
struct CalibrationSuggestion {
    fail_threshold: f64,
    warn_factor: f64,
    noise_threshold: f64,
    noise_policy: perfgate_types::NoisePolicy,
    recommended_repeat: usize,
    suggest_paired: bool,
}

pub(crate) fn execute_calibrate(args: CalibrateArgs) -> anyhow::Result<()> {
    let config = load_config_file(&args.config)?;
    config
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;
    let bench = config
        .benches
        .iter()
        .find(|bench| bench.name == args.bench)
        .ok_or_else(|| {
            ConfigValidationError::BenchName(format!("bench '{}' not found in config", args.bench))
        })?;

    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&config));
    let run_path = args
        .run
        .clone()
        .or_else(|| find_calibration_run_path(&out_dir, &args.bench));
    let run_receipt = run_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| read_json::<RunReceipt>(path))
        .transpose()?;

    let baseline_path = resolve_baseline_path(&args.baseline, &args.bench, &config);
    let baseline_receipt = load_optional_baseline_receipt(&baseline_path)?;
    let evidence_receipt = run_receipt.as_ref().or(baseline_receipt.as_ref());
    let cv = run_receipt
        .as_ref()
        .and_then(|receipt| receipt.stats.wall_ms.cv())
        .or_else(|| {
            baseline_receipt
                .as_ref()
                .and_then(|receipt| receipt.stats.wall_ms.cv())
        });
    let sample_count = evidence_receipt
        .map(measured_sample_count)
        .unwrap_or_default();
    let configured_threshold = configured_wall_threshold(&config, bench);
    let suggestion = suggest_calibration(cv, sample_count, configured_threshold);

    println!("perfgate calibrate");
    println!();
    println!("Bench: {}", args.bench);
    if sample_count == 0 {
        println!("Samples: unavailable");
    } else {
        println!(
            "Samples: {sample_count} measured sample{}",
            plural(sample_count)
        );
    }
    println!(
        "CV: {}",
        cv.map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string())
    );
    let evidence_host_class = evidence_receipt
        .map(host_class)
        .unwrap_or_else(|| "unknown".to_string());
    println!("Host class: {evidence_host_class}");
    println!();
    println!("Evidence:");
    if let Some(path) = run_path.as_ref().filter(|path| path.exists()) {
        println!("  run: {}", path.display());
    } else if let Some(path) = run_path.as_ref() {
        println!("  run: missing ({})", path.display());
    } else {
        println!(
            "  run: missing (expected {})",
            out_dir.join(RUN_RECEIPT_FILE).display()
        );
    }
    if baseline_receipt.is_some() {
        println!("  baseline: {}", baseline_path.display());
    } else {
        println!("  baseline: missing ({})", baseline_path.display());
    }
    println!();
    println!(
        "Suggested fail threshold: {}",
        format_percent(suggestion.fail_threshold)
    );
    println!(
        "Suggested warn threshold: {}",
        format_percent(suggestion.fail_threshold * suggestion.warn_factor)
    );
    println!(
        "Suggested noise threshold: {}",
        format_percent(suggestion.noise_threshold)
    );
    println!(
        "Suggested noise policy: {}",
        suggestion.noise_policy.as_str()
    );
    println!(
        "Repeat guidance: collect at least {} measured samples before tightening.",
        suggestion.recommended_repeat
    );
    if suggestion.suggest_paired {
        println!("Paired mode: recommended before making this gate blocking.");
    } else {
        println!("Paired mode: not required yet; use it if reviewers see inconsistent results.");
    }
    println!();
    println!("Suggested config patch:");
    println!("  threshold = {:.2}", suggestion.fail_threshold);
    println!("  warn_factor = {:.2}", suggestion.warn_factor);
    println!("  noise_threshold = {:.2}", suggestion.noise_threshold);
    println!("  noise_policy = \"{}\"", suggestion.noise_policy.as_str());
    if args.emit_patch {
        print_calibration_patch(&suggestion, sample_count, cv, &evidence_host_class);
    } else {
        println!("  run with --emit-patch for a reasoned, copy-ready TOML fragment");
    }
    println!();
    println!("Next:");
    if run_receipt.is_none() {
        println!(
            "  {}",
            check_command(&args.config, Some(&args.bench), false)
        );
    }
    println!("  {}", check_command(&args.config, Some(&args.bench), true));
    if suggestion.suggest_paired {
        println!("  {}", paired_command(Some(&args.bench)));
    }
    println!("Do not:");
    println!("  do not auto-edit thresholds from this advisory output; review the benchmark first");
    println!();
    println!("Advisory only: no config was written.");

    Ok(())
}

fn print_calibration_patch(
    suggestion: &CalibrationSuggestion,
    sample_count: usize,
    cv: Option<f64>,
    host_class: &str,
) {
    println!();
    println!("Reviewable TOML patch:");
    if sample_count == 0 {
        println!("# Suggested without local samples on {host_class}; collect receipts first.");
    } else {
        println!(
            "# Suggested from {sample_count} measured sample{} on {host_class}.",
            plural(sample_count)
        );
    }
    println!(
        "# CV: {}; {}.",
        cv.map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string()),
        calibration_patch_summary(suggestion)
    );
    println!("threshold = {:.2}", suggestion.fail_threshold);
    println!("warn_factor = {:.2}", suggestion.warn_factor);
    println!("noise_threshold = {:.2}", suggestion.noise_threshold);
    println!("noise_policy = \"{}\"", suggestion.noise_policy.as_str());
    println!("repeat = {}", suggestion.recommended_repeat);
    println!();
    println!("Reasons:");
    println!(
        "  samples: {}",
        if sample_count == 0 {
            "unavailable".to_string()
        } else {
            format!("{sample_count} measured sample{}", plural(sample_count))
        }
    );
    println!(
        "  noise: {}",
        cv.map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!("  host: {host_class}");
    if suggestion.suggest_paired {
        println!("  paired mode: recommended before blocking");
    } else {
        println!("  paired mode: not required by current evidence");
    }
    println!();
    println!("When not to apply:");
    println!("  benchmark is not the workload reviewers want to gate");
    println!("  samples are missing, too few, or collected on the wrong host class");
    println!("  paired mode is recommended but has not been run");
}

fn calibration_patch_summary(suggestion: &CalibrationSuggestion) -> &'static str {
    if suggestion.suggest_paired {
        "keep advisory or use paired mode before required CI"
    } else {
        "review before applying to defaults or a specific benchmark budget"
    }
}

fn find_calibration_run_path(out_dir: &Path, bench_name: &str) -> Option<PathBuf> {
    [
        out_dir.join(bench_name).join(RUN_RECEIPT_FILE),
        out_dir.join(RUN_RECEIPT_FILE),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn configured_wall_threshold(config: &ConfigFile, bench: &perfgate_types::BenchConfigFile) -> f64 {
    bench
        .budgets
        .as_ref()
        .and_then(|budgets| budgets.get(&Metric::WallMs))
        .and_then(|budget| budget.threshold)
        .or(config.defaults.threshold)
        .unwrap_or(0.20)
}

fn suggest_calibration(
    cv: Option<f64>,
    sample_count: usize,
    configured_threshold: f64,
) -> CalibrationSuggestion {
    let fail_threshold = cv
        .map(|cv| {
            if cv <= 0.02 {
                0.05
            } else if cv <= 0.05 {
                0.10
            } else if cv <= 0.10 {
                0.15
            } else if cv <= 0.20 {
                0.20
            } else {
                configured_threshold.max(0.30)
            }
        })
        .unwrap_or(configured_threshold.max(0.20));
    let noise_threshold = cv.map(|cv| (cv * 2.0).clamp(0.05, 0.30)).unwrap_or(0.08);
    CalibrationSuggestion {
        fail_threshold,
        warn_factor: 0.50,
        noise_threshold,
        noise_policy: perfgate_types::NoisePolicy::Warn,
        recommended_repeat: sample_count.max(10),
        suggest_paired: cv.is_some_and(|cv| cv > 0.10),
    }
}

fn measured_sample_count(receipt: &RunReceipt) -> usize {
    receipt
        .samples
        .iter()
        .filter(|sample| !sample.warmup)
        .count()
}

fn host_class(receipt: &RunReceipt) -> String {
    format!("{}-{}", receipt.run.host.os, receipt.run.host.arch)
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignalRecommendation {
    SafeToGate,
    AdvisoryOnly,
    IncreaseSamples,
    UsePairedMode,
    RefreshBaseline,
    CheckHostMismatch,
    NoDecisionYet,
}

impl SignalRecommendation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SafeToGate => "safe_to_gate",
            Self::AdvisoryOnly => "advisory_only",
            Self::IncreaseSamples => "increase_samples",
            Self::UsePairedMode => "use_paired_mode",
            Self::RefreshBaseline => "refresh_baseline",
            Self::CheckHostMismatch => "check_host_mismatch",
            Self::NoDecisionYet => "no_decision_yet",
        }
    }

    pub(crate) fn meaning(self) -> &'static str {
        match self {
            Self::SafeToGate => "signal looks stable enough for required-baseline checks",
            Self::AdvisoryOnly => {
                "evidence exists but is not complete enough to make blocking boring"
            }
            Self::IncreaseSamples => "collect more measured samples before tightening or blocking",
            Self::UsePairedMode => {
                "ordinary runs are noisy; compare baseline/current under paired conditions"
            }
            Self::RefreshBaseline => "baseline is stale enough to refresh before relying on it",
            Self::CheckHostMismatch => {
                "baseline/current host classes differ; rerun on a compatible host"
            }
            Self::NoDecisionYet => {
                "setup or receipts are incomplete; do not treat this as a regression"
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct SignalDoctorRow {
    pub(crate) bench: String,
    pub(crate) run_path: PathBuf,
    pub(crate) baseline_path: PathBuf,
    pub(crate) compare_path: PathBuf,
    pub(crate) run_found: bool,
    pub(crate) baseline_found: bool,
    pub(crate) baseline_remote: bool,
    pub(crate) compare_found: bool,
    pub(crate) samples: usize,
    pub(crate) cv: Option<f64>,
    pub(crate) host_stability: String,
    pub(crate) baseline_age_days: Option<i64>,
    pub(crate) recent_drift: String,
    pub(crate) recommendation: SignalRecommendation,
}

pub(crate) fn execute_signal_doctor(args: SignalDoctorArgs) -> anyhow::Result<()> {
    let config = load_config_file(&args.config)?;
    config
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;
    let benches = configured_signal_benches(&config, args.bench.as_deref())?;
    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&config));

    println!("perfgate doctor signal");
    println!();
    if benches.is_empty() {
        println!("No benchmarks are configured.");
        println!("Next:");
        println!(
            "  edit {} and add a reviewed [[bench]] command",
            args.config.display()
        );
        println!("Do not:");
        println!("  promote baselines until the benchmark measures the workload you care about");
        return Ok(());
    }

    let mut counts = SignalDoctorCounts::default();
    for bench_name in &benches {
        let row = inspect_signal(&config, &out_dir, bench_name)?;
        counts.record(row.recommendation);
        print_signal_row(&row, &args.config);
    }

    println!();
    println!(
        "Summary: {} safe_to_gate, {} advisory_only, {} increase_samples, {} use_paired_mode, {} refresh_baseline, {} check_host_mismatch, {} no_decision_yet",
        counts.safe_to_gate,
        counts.advisory_only,
        counts.increase_samples,
        counts.use_paired_mode,
        counts.refresh_baseline,
        counts.check_host_mismatch,
        counts.no_decision_yet
    );
    println!();
    println!("Do not:");
    println!("  treat noisy or immature evidence as policy just because receipts exist");
    println!("  make server ledger upload part of local correctness");
    println!();
    println!("Advisory only: no config, baseline, threshold, or policy was changed.");

    Ok(())
}

#[derive(Default)]
struct SignalDoctorCounts {
    safe_to_gate: usize,
    advisory_only: usize,
    increase_samples: usize,
    use_paired_mode: usize,
    refresh_baseline: usize,
    check_host_mismatch: usize,
    no_decision_yet: usize,
}

impl SignalDoctorCounts {
    fn record(&mut self, recommendation: SignalRecommendation) {
        match recommendation {
            SignalRecommendation::SafeToGate => self.safe_to_gate += 1,
            SignalRecommendation::AdvisoryOnly => self.advisory_only += 1,
            SignalRecommendation::IncreaseSamples => self.increase_samples += 1,
            SignalRecommendation::UsePairedMode => self.use_paired_mode += 1,
            SignalRecommendation::RefreshBaseline => self.refresh_baseline += 1,
            SignalRecommendation::CheckHostMismatch => self.check_host_mismatch += 1,
            SignalRecommendation::NoDecisionYet => self.no_decision_yet += 1,
        }
    }
}

fn print_signal_row(row: &SignalDoctorRow, config_path: &Path) {
    println!("bench: {}", row.bench);
    println!(
        "samples: {} measured sample{}",
        row.samples,
        plural(row.samples)
    );
    println!(
        "cv: {}",
        row.cv
            .map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!("host stability: {}", row.host_stability);
    println!(
        "baseline age: {}",
        row.baseline_age_days
            .map(|days| format!("{days} day{}", plural(days as usize)))
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("recent drift: {}", row.recent_drift);
    println!("recommendation: {}", row.recommendation.as_str());
    println!("meaning: {}", row.recommendation.meaning());
    println!("artifacts:");
    println!(
        "  run: {}{}",
        row.run_path.display(),
        if row.run_found { "" } else { " (missing)" }
    );
    if row.baseline_remote {
        println!(
            "  baseline: {} (remote, not probed)",
            row.baseline_path.display()
        );
    } else {
        println!(
            "  baseline: {}{}",
            row.baseline_path.display(),
            if row.baseline_found { "" } else { " (missing)" }
        );
    }
    println!(
        "  compare: {}{}",
        row.compare_path.display(),
        if row.compare_found { "" } else { " (missing)" }
    );
    println!("next:");
    for command in signal_next_commands(row, config_path) {
        println!("  {command}");
    }
    println!();
}

fn signal_next_commands(row: &SignalDoctorRow, config_path: &Path) -> Vec<String> {
    match row.recommendation {
        SignalRecommendation::SafeToGate => {
            vec![check_command(config_path, Some(&row.bench), true)]
        }
        SignalRecommendation::UsePairedMode => vec![
            format!(
                "perfgate calibrate --config {} --bench {}",
                config_path.display(),
                row.bench
            ),
            paired_command(Some(&row.bench)),
        ],
        SignalRecommendation::IncreaseSamples => vec![
            check_command(config_path, Some(&row.bench), false),
            format!(
                "perfgate calibrate --config {} --bench {}",
                config_path.display(),
                row.bench
            ),
        ],
        SignalRecommendation::RefreshBaseline => vec![
            check_command(config_path, Some(&row.bench), true),
            format!("review and refresh baseline for {}", row.bench),
        ],
        SignalRecommendation::CheckHostMismatch => vec![
            "rerun on the same runner class as the baseline".to_string(),
            check_command(config_path, Some(&row.bench), true),
        ],
        SignalRecommendation::AdvisoryOnly => vec![
            check_command(config_path, Some(&row.bench), false),
            format!(
                "perfgate baseline doctor --config {} --bench {}",
                config_path.display(),
                row.bench
            ),
        ],
        SignalRecommendation::NoDecisionYet => vec![
            check_command(config_path, Some(&row.bench), false),
            format!(
                "perfgate baseline promote --config {} --bench {}",
                config_path.display(),
                row.bench
            ),
        ],
    }
}

pub(crate) fn inspect_signal(
    config: &ConfigFile,
    out_dir: &Path,
    bench_name: &str,
) -> anyhow::Result<SignalDoctorRow> {
    let run_path = signal_run_path(out_dir, bench_name);
    let run_receipt = if run_path.exists() {
        Some(read_json::<RunReceipt>(&run_path)?)
    } else {
        None
    };

    let baseline_path = resolve_baseline_path(&None, bench_name, config);
    let baseline_text = baseline_path.to_string_lossy();
    let baseline_remote = is_remote_storage_uri(&baseline_text);
    let baseline_receipt = if baseline_remote {
        None
    } else {
        load_optional_baseline_receipt(&baseline_path)?
    };

    let compare_path = signal_compare_path(out_dir, bench_name);
    let compare_receipt = if compare_path.exists() {
        Some(read_json::<CompareReceipt>(&compare_path)?)
    } else {
        None
    };

    let samples = run_receipt
        .as_ref()
        .or(baseline_receipt.as_ref())
        .map(measured_sample_count)
        .unwrap_or_default();
    let cv = run_receipt
        .as_ref()
        .and_then(|receipt| receipt.stats.wall_ms.cv())
        .or_else(|| compare_receipt.as_ref().and_then(compare_cv))
        .or_else(|| {
            baseline_receipt
                .as_ref()
                .and_then(|receipt| receipt.stats.wall_ms.cv())
        });
    let (host_stability, host_mismatch) =
        signal_host_stability(run_receipt.as_ref(), baseline_receipt.as_ref());
    let baseline_age_days = baseline_receipt.as_ref().and_then(baseline_age_days);
    let recent_drift = compare_receipt
        .as_ref()
        .map(signal_recent_drift)
        .unwrap_or_else(|| "missing compare receipt".to_string());
    let recommendation = signal_recommendation(SignalRecommendationInput {
        baseline_found: baseline_receipt.is_some(),
        baseline_remote,
        compare_found: compare_receipt.is_some(),
        samples,
        cv,
        host_mismatch,
        baseline_age_days,
    });

    Ok(SignalDoctorRow {
        bench: bench_name.to_string(),
        run_path,
        baseline_path,
        compare_path,
        run_found: run_receipt.is_some(),
        baseline_found: baseline_receipt.is_some(),
        baseline_remote,
        compare_found: compare_receipt.is_some(),
        samples,
        cv,
        host_stability,
        baseline_age_days,
        recent_drift,
        recommendation,
    })
}

struct SignalRecommendationInput {
    baseline_found: bool,
    baseline_remote: bool,
    compare_found: bool,
    samples: usize,
    cv: Option<f64>,
    host_mismatch: bool,
    baseline_age_days: Option<i64>,
}

fn signal_recommendation(input: SignalRecommendationInput) -> SignalRecommendation {
    if !input.baseline_found && !input.baseline_remote {
        return SignalRecommendation::NoDecisionYet;
    }
    if input.host_mismatch {
        return SignalRecommendation::CheckHostMismatch;
    }
    if input
        .baseline_age_days
        .is_some_and(|days| days > SIGNAL_STALE_BASELINE_DAYS)
    {
        return SignalRecommendation::RefreshBaseline;
    }
    if input.cv.is_some_and(|cv| cv > SIGNAL_HIGH_NOISE_CV) {
        return SignalRecommendation::UsePairedMode;
    }
    if input.samples < SIGNAL_MATURE_SAMPLE_LIMIT {
        return SignalRecommendation::IncreaseSamples;
    }
    if !input.compare_found || input.baseline_remote {
        return SignalRecommendation::AdvisoryOnly;
    }
    SignalRecommendation::SafeToGate
}

fn configured_signal_benches(
    config: &ConfigFile,
    bench: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(bench) = bench {
        if config
            .benches
            .iter()
            .any(|candidate| candidate.name == bench)
        {
            return Ok(vec![bench.to_string()]);
        }
        return Err(ConfigValidationError::BenchName(format!(
            "bench '{}' not found in config",
            bench
        ))
        .into());
    }

    Ok(config
        .benches
        .iter()
        .map(|bench| bench.name.clone())
        .collect())
}

fn signal_run_path(out_dir: &Path, bench_name: &str) -> PathBuf {
    let per_bench = out_dir.join(bench_name).join(RUN_RECEIPT_FILE);
    if per_bench.exists() {
        per_bench
    } else {
        out_dir.join(RUN_RECEIPT_FILE)
    }
}

fn signal_compare_path(out_dir: &Path, bench_name: &str) -> PathBuf {
    let per_bench = out_dir.join(bench_name).join(COMPARE_RECEIPT_FILE);
    if per_bench.exists() {
        per_bench
    } else {
        out_dir.join(COMPARE_RECEIPT_FILE)
    }
}

fn compare_cv(compare: &CompareReceipt) -> Option<f64> {
    compare
        .deltas
        .values()
        .filter_map(|delta| delta.cv)
        .max_by(|left, right| left.total_cmp(right))
}

fn signal_recent_drift(compare: &CompareReceipt) -> String {
    let status = compare.verdict.status.as_str();
    let largest_regression = compare
        .deltas
        .iter()
        .max_by(|(_, left), (_, right)| left.regression.total_cmp(&right.regression));

    if let Some((metric, delta)) = largest_regression {
        format!(
            "{} ({} regression {})",
            status,
            metric.as_str(),
            format_percent(delta.regression)
        )
    } else {
        format!("{status} (no metric deltas)")
    }
}

fn signal_host_stability(
    run: Option<&RunReceipt>,
    baseline: Option<&RunReceipt>,
) -> (String, bool) {
    match (run, baseline) {
        (Some(run), Some(baseline)) => {
            let run_host = host_class(run);
            let baseline_host = host_class(baseline);
            if run_host == baseline_host {
                (format!("stable ({run_host})"), false)
            } else {
                (
                    format!("mismatch (baseline {baseline_host}, current {run_host})"),
                    true,
                )
            }
        }
        (None, Some(baseline)) => {
            let baseline_host = host_class(baseline);
            let current_host = current_host_class();
            if baseline_host == current_host {
                (format!("baseline-only ({baseline_host})"), false)
            } else {
                (
                    format!("mismatch (baseline {baseline_host}, current {current_host})"),
                    true,
                )
            }
        }
        (Some(run), None) => (format!("run-only ({})", host_class(run)), false),
        (None, None) => ("unknown".to_string(), false),
    }
}

fn current_host_class() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn baseline_age_days(receipt: &RunReceipt) -> Option<i64> {
    let started_at = DateTime::parse_from_rfc3339(&receipt.run.started_at).ok()?;
    let age = Utc::now().signed_duration_since(started_at.with_timezone(&Utc));
    Some(age.num_days().max(0))
}

pub(crate) fn execute_doctor(args: DoctorArgs, server_flags: ServerFlags) -> anyhow::Result<()> {
    let mut checks = Vec::new();
    checks.push(DoctorCheck::ok(
        "version",
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    checks.push(DoctorCheck::ok(
        "platform",
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    ));

    let config = if args.config.exists() {
        match load_config_file(&args.config) {
            Ok(config) => {
                match config.validate() {
                    Ok(()) => checks.push(DoctorCheck::ok(
                        "config",
                        format!(
                            "{} found ({} benchmark{})",
                            args.config.display(),
                            config.benches.len(),
                            plural(config.benches.len())
                        ),
                    )),
                    Err(error) => checks.push(DoctorCheck::fail(
                        "config",
                        format!("{} is invalid: {error}", args.config.display()),
                    )),
                }
                Some(config)
            }
            Err(error) => {
                checks.push(DoctorCheck::fail(
                    "config",
                    format!("failed to load {}: {error}", args.config.display()),
                ));
                None
            }
        }
    } else {
        checks.push(DoctorCheck::fail(
            "config",
            format!(
                "{} not found; run `perfgate init` or pass --config",
                args.config.display()
            ),
        ));
        None
    };

    checks.push(doctor_git_check());
    checks.push(doctor_ci_check());

    if let Some(config) = &config {
        checks.push(doctor_benchmark_commands(config));
        checks.push(doctor_baselines(config, &server_flags));
        checks.push(doctor_server(config, &server_flags));
    } else {
        checks.push(DoctorCheck::warn(
            "benchmarks",
            "skipped because config could not be loaded",
        ));
        checks.push(DoctorCheck::warn(
            "baselines",
            "skipped because config could not be loaded",
        ));
        checks.push(doctor_server(&ConfigFile::default(), &server_flags));
    }

    let artifact_dir = resolve_configured_out_dir(args.out_dir.as_ref(), config.as_ref());
    checks.push(doctor_artifact_dir(&artifact_dir));
    let adoption_state = classify_adoption_state(config.as_ref(), &args.config, &server_flags);

    println!("perfgate doctor");
    println!();
    for check in &checks {
        println!(
            "{:<4} {:<18} {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }
    print_adoption_state(adoption_state, &args.config);

    let failed = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();
    let warned = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Warn)
        .count();
    println!();
    println!(
        "Summary: {failed} failed, {warned} warning{}",
        plural(warned)
    );

    if args.strict && failed > 0 {
        anyhow::bail!("doctor found {failed} failed check{}", plural(failed));
    }

    Ok(())
}

pub(crate) fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn doctor_git_check() -> DoctorCheck {
    match run_git_capture(&["rev-parse", "--is-inside-work-tree"]) {
        Some(value) if value == "true" => {
            let branch = run_git_capture(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|| "unknown".to_string());
            DoctorCheck::ok("git", format!("repository detected ({branch})"))
        }
        _ => DoctorCheck::ok("git", "not a git repository"),
    }
}

fn doctor_ci_check() -> DoctorCheck {
    match detect_ci_provider() {
        Some(provider) => DoctorCheck::ok("ci", format!("detected {provider}")),
        None => DoctorCheck::ok("ci", "not detected"),
    }
}

fn detect_ci_provider() -> Option<&'static str> {
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        Some("GitHub Actions")
    } else if std::env::var_os("GITLAB_CI").is_some() {
        Some("GitLab CI")
    } else if std::env::var_os("BITBUCKET_BUILD_NUMBER").is_some() {
        Some("Bitbucket Pipelines")
    } else if std::env::var_os("CIRCLECI").is_some() {
        Some("CircleCI")
    } else if std::env::var_os("CI").is_some() {
        Some("generic CI")
    } else {
        None
    }
}

fn doctor_benchmark_commands(config: &ConfigFile) -> DoctorCheck {
    if config.benches.is_empty() {
        return DoctorCheck::fail("benchmarks", "no [[bench]] entries configured");
    }

    let mut runnable = 0usize;
    let mut missing = Vec::new();
    for bench in &config.benches {
        match bench_command_runnable(bench) {
            Ok(true) => runnable += 1,
            Ok(false) => missing.push(bench.name.clone()),
            Err(error) => missing.push(format!("{} ({error})", bench.name)),
        }
    }

    if missing.is_empty() {
        DoctorCheck::ok(
            "benchmarks",
            format!(
                "{}/{} command{} runnable",
                runnable,
                config.benches.len(),
                plural(config.benches.len())
            ),
        )
    } else {
        DoctorCheck::fail(
            "benchmarks",
            format!(
                "{}/{} command{} runnable; not runnable: {}",
                runnable,
                config.benches.len(),
                plural(config.benches.len()),
                missing.join(", ")
            ),
        )
    }
}

fn bench_command_runnable(bench: &perfgate_types::BenchConfigFile) -> anyhow::Result<bool> {
    let Some(program) = bench.command.first() else {
        return Ok(false);
    };

    let cwd = resolve_bench_cwd(bench.cwd.as_deref());
    if !cwd.exists() {
        anyhow::bail!("cwd does not exist: {}", cwd.display());
    }

    Ok(program_is_runnable(program, &cwd))
}

fn resolve_bench_cwd(cwd: Option<&str>) -> PathBuf {
    match cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => PathBuf::from("."),
    }
}

fn program_is_runnable(program: &str, cwd: &Path) -> bool {
    let path = Path::new(program);
    if path.is_absolute() || program.contains('/') || program.contains('\\') {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        return executable_candidate_exists(&candidate);
    }

    find_program_on_path(program).is_some()
}

fn find_program_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if executable_candidate_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn executable_candidate_exists(path: &Path) -> bool {
    if path.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let Ok(metadata) = path.metadata() else {
                return false;
            };
            return metadata.permissions().mode() & 0o111 != 0;
        }

        #[cfg(not(unix))]
        {
            return true;
        }
    }

    #[cfg(windows)]
    {
        if path.extension().is_none() {
            let pathext =
                std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
            for ext in pathext.split(';').filter(|ext| !ext.is_empty()) {
                let ext = ext.trim_start_matches('.');
                if path.with_extension(ext).is_file() {
                    return true;
                }
            }
        }
    }

    false
}

fn doctor_baselines(config: &ConfigFile, server_flags: &ServerFlags) -> DoctorCheck {
    if config.benches.is_empty() {
        return DoctorCheck::warn("baselines", "skipped because no benchmarks are configured");
    }

    let server_config = server_flags.resolve(&config.baseline_server);
    if server_config.is_configured() {
        return DoctorCheck::ok("baselines", "baseline server configured");
    }

    let mut local = 0usize;
    let mut found = 0usize;
    let mut remote = 0usize;
    for bench in &config.benches {
        let path = resolve_baseline_path(&None, &bench.name, config);
        let path_text = path.to_string_lossy();
        if is_remote_storage_uri(&path_text) {
            remote += 1;
            continue;
        }
        local += 1;
        if path.exists() {
            found += 1;
        }
    }

    if local == 0 && remote > 0 {
        return DoctorCheck::ok(
            "baselines",
            format!(
                "{remote} remote baseline URI{} configured; not probed",
                plural(remote)
            ),
        );
    }

    if found == local {
        DoctorCheck::ok(
            "baselines",
            format!("{found}/{local} local baseline{} found", plural(local)),
        )
    } else {
        DoctorCheck::warn(
            "baselines",
            format!(
                "{found}/{local} local baseline{} found; inspect with `perfgate baseline status` and create missing baselines with `perfgate baseline promote --bench <bench>`",
                plural(local)
            ),
        )
    }
}

fn doctor_server(config: &ConfigFile, server_flags: &ServerFlags) -> DoctorCheck {
    let server_config = server_flags.resolve(&config.baseline_server);
    if !server_config.is_configured() {
        return DoctorCheck::ok("baseline server", "not configured");
    }

    let Some(url) = server_config.url.as_ref() else {
        return DoctorCheck::ok("baseline server", "not configured");
    };

    let mut client_config = ClientConfig::new(url)
        .with_timeout(Duration::from_secs(2))
        .with_retry(RetryConfig::new().with_max_retries(0));
    if let Some(api_key) = &server_config.api_key {
        client_config = client_config.with_api_key(api_key);
    }

    let client = match BaselineClient::new(client_config) {
        Ok(client) => client,
        Err(error) => {
            return DoctorCheck::fail("baseline server", format!("{url} invalid: {error}"));
        }
    };

    match with_tokio_runtime(async { client.health_check().await.map_err(anyhow::Error::from) }) {
        Ok(health) if health.status == "healthy" => {
            let project = server_config.project.as_deref().unwrap_or("not configured");
            DoctorCheck::ok(
                "baseline server",
                format!("{url} reachable (project: {project})"),
            )
        }
        Ok(health) => DoctorCheck::fail(
            "baseline server",
            format!("{url} returned unhealthy status: {}", health.status),
        ),
        Err(error) => {
            DoctorCheck::fail("baseline server", format!("{url} not reachable: {error:#}"))
        }
    }
}

fn doctor_artifact_dir(out_dir: &Path) -> DoctorCheck {
    match ensure_artifact_dir_writable(out_dir) {
        Ok(()) => DoctorCheck::ok(
            "artifact directory",
            format!("{} writable", out_dir.display()),
        ),
        Err(error) => DoctorCheck::fail(
            "artifact directory",
            format!("{} not writable: {error}", out_dir.display()),
        ),
    }
}

pub(crate) fn ensure_artifact_dir_writable(out_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let probe = out_dir.join(".perfgate-doctor-write-test");
    fs::write(&probe, b"perfgate doctor\n")?;
    fs::remove_file(probe)?;
    Ok(())
}
