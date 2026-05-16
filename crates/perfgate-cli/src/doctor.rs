//! Doctor and calibration command implementation.
//!
//! This module keeps setup-health diagnostics and benchmark-threshold calibration
//! separate from argument parsing and command dispatch in `main.rs`.

use crate::{
    CalibrateArgs, DoctorArgs, RUN_RECEIPT_FILE, ServerFlags, check_command,
    load_optional_baseline_receipt, paired_command, read_json, resolve_configured_out_dir,
    run_git_capture, with_tokio_runtime,
};
use perfgate::app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate::app::init::{CiPlatform, ci_workflow_path};
use perfgate_client::{BaselineClient, ClientConfig, RetryConfig};
use perfgate_types::config::load_config_file;
use perfgate_types::error::ConfigValidationError;
use perfgate_types::{ConfigFile, Metric, RunReceipt};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    println!(
        "Host class: {}",
        evidence_receipt
            .map(host_class)
            .unwrap_or_else(|| "unknown".to_string())
    );
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
