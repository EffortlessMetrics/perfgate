use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use schemars::schema_for;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_FILES: [&str; 5] = [
    "perfgate.run.v1.schema.json",
    "perfgate.compare.v1.schema.json",
    "perfgate.config.v1.schema.json",
    "perfgate.report.v1.schema.json",
    "sensor.report.v1.schema.json",
];

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repo automation for perfgate")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

/// Supported crates for mutation testing
#[derive(Debug, Clone, Copy, ValueEnum)]
enum MutantsCrate {
    #[value(name = "perfgate-domain")]
    Domain,
    #[value(name = "perfgate-types")]
    Types,
    #[value(name = "perfgate-app")]
    App,
    #[value(name = "perfgate-adapters")]
    Adapters,
    #[value(name = "perfgate-cli")]
    Cli,
    #[value(name = "perfgate-sha256")]
    Sha256,
    #[value(name = "perfgate-stats")]
    Stats,
    #[value(name = "perfgate-validation")]
    Validation,
    #[value(name = "perfgate-host-detect")]
    HostDetect,
    #[value(name = "perfgate-export")]
    Export,
    #[value(name = "perfgate-render")]
    Render,
    #[value(name = "perfgate-sensor")]
    Sensor,
    #[value(name = "perfgate-paired")]
    Paired,
    #[value(name = "perfgate-fake")]
    Fake,
}

impl MutantsCrate {
    fn as_package_name(&self) -> &'static str {
        match self {
            MutantsCrate::Domain => "perfgate-domain",
            MutantsCrate::Types => "perfgate-types",
            MutantsCrate::App => "perfgate-app",
            MutantsCrate::Adapters => "perfgate-adapters",
            MutantsCrate::Cli => "perfgate-cli",
            MutantsCrate::Sha256 => "perfgate-sha256",
            MutantsCrate::Stats => "perfgate-stats",
            MutantsCrate::Validation => "perfgate-validation",
            MutantsCrate::HostDetect => "perfgate-host-detect",
            MutantsCrate::Export => "perfgate-export",
            MutantsCrate::Render => "perfgate-render",
            MutantsCrate::Sensor => "perfgate-sensor",
            MutantsCrate::Paired => "perfgate-paired",
            MutantsCrate::Fake => "perfgate-fake",
        }
    }

    fn target_kill_rate(&self) -> u8 {
        match self {
            MutantsCrate::Domain => 100,
            MutantsCrate::Types => 95,
            MutantsCrate::App => 90,
            MutantsCrate::Adapters => 80,
            MutantsCrate::Cli => 70,
            MutantsCrate::Sha256 => 100,
            MutantsCrate::Stats => 100,
            MutantsCrate::Validation => 100,
            MutantsCrate::HostDetect => 100,
            MutantsCrate::Export => 90,
            MutantsCrate::Render => 90,
            MutantsCrate::Sensor => 90,
            MutantsCrate::Paired => 100,
            MutantsCrate::Fake => 70,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// (Re)generate JSON Schemas for receipts and config.
    Schema {
        /// Output directory
        #[arg(long, default_value = "schemas")]
        out_dir: PathBuf,
    },

    /// Verify committed schemas are locked to generated output (byte-for-byte).
    SchemaCheck {
        /// Schemas directory to verify
        #[arg(long, default_value = "schemas")]
        schemas_dir: PathBuf,
    },

    /// Run the "usual" repo checks (fmt, clippy, test, schema-check, conform).
    Ci,

    /// Validate JSON fixtures against the vendored sensor.report.v1 schema.
    Conform {
        /// Directory of fixtures to validate (default: golden fixtures)
        #[arg(long)]
        fixtures: Option<PathBuf>,

        /// Validate a single file
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Copy golden fixtures to contracts/fixtures/ (golden is source of truth).
    SyncFixtures,

    /// Run mutation testing via cargo-mutants (must be installed).
    Mutants {
        /// Run mutation testing on a specific crate only
        #[arg(long = "crate", value_enum)]
        crate_name: Option<MutantsCrate>,

        /// Generate a summary report after mutation testing
        #[arg(long)]
        summary: bool,

        /// Extra args forwarded to cargo-mutants
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// List all microcrates and their purposes.
    Microcrates,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Schema { out_dir } => cmd_schema(&out_dir),
        Command::SchemaCheck { schemas_dir } => cmd_schema_check(&schemas_dir),
        Command::Ci => cmd_ci(),
        Command::Conform { fixtures, file } => cmd_conform(fixtures, file),
        Command::SyncFixtures => cmd_sync_fixtures(),
        Command::Mutants {
            crate_name,
            summary,
            args,
        } => cmd_mutants(crate_name, summary, args),
        Command::Microcrates => cmd_microcrates(),
    }
}

fn cmd_ci() -> anyhow::Result<()> {
    let target_dir =
        std::env::var("PERFGATE_CI_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let cargo_env = vec![("CARGO_TARGET_DIR", target_dir.as_str())];
    let xtask_target_dir = format!("{target_dir}/xtask-self");
    let xtask_env = vec![("CARGO_TARGET_DIR", xtask_target_dir.as_str())];

    run_with_env("cargo", ["fmt", "--all", "--", "--check"], &cargo_env)?;
    run_with_env(
        "cargo",
        [
            "clippy",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &cargo_env,
    )?;
    run_with_env(
        "cargo",
        [
            "test",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-features",
        ],
        &cargo_env,
    )?;
    run_with_env(
        "cargo",
        [
            "clippy",
            "-p",
            "xtask",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &xtask_env,
    )?;
    run_with_env(
        "cargo",
        ["test", "-p", "xtask", "--all-features"],
        &xtask_env,
    )?;
    cmd_schema_check(Path::new("schemas"))?;
    cmd_conform(None, None)?;
    Ok(())
}

fn cmd_conform(fixtures_dir: Option<PathBuf>, single_file: Option<PathBuf>) -> anyhow::Result<()> {
    let is_default_run = fixtures_dir.is_none() && single_file.is_none();

    // Load vendored schema
    let schema_path = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = fs::read_to_string(&schema_path)
        .with_context(|| format!("read {}", schema_path.display()))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_content).context("parse vendored schema")?;
    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| anyhow::anyhow!("compile schema: {}", e))?;

    let mut files_to_validate: Vec<PathBuf> = Vec::new();

    if let Some(path) = single_file {
        files_to_validate.push(path);
    } else if let Some(dir) = fixtures_dir {
        // Third-party mode: validate every JSON file in the provided directory.
        files_to_validate.extend(collect_json_files(&dir, None)?);
    } else {
        // Default: validate known sensor_report fixtures in golden + contracts dirs.
        let default_dirs = [
            PathBuf::from("crates/perfgate-cli/tests/fixtures/golden"),
            PathBuf::from("contracts/fixtures"),
        ];

        for dir in &default_dirs {
            files_to_validate.extend(collect_json_files(dir, Some("sensor_report_"))?);
        }
    }

    if files_to_validate.is_empty() {
        anyhow::bail!("no fixture files found to validate");
    }

    files_to_validate.sort();

    let mut errors = 0u32;
    for path in &files_to_validate {
        let content =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let instance: serde_json::Value =
            serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))?;

        let validation_errors: Vec<_> = validator.iter_errors(&instance).collect();
        if validation_errors.is_empty() {
            println!("  OK  {}", path.display());
        } else {
            errors += 1;
            println!("  FAIL  {}", path.display());
            for err in &validation_errors {
                println!("        - {}", err);
            }
        }
    }

    println!(
        "\nValidated {} files, {} errors",
        files_to_validate.len(),
        errors
    );

    if errors > 0 {
        anyhow::bail!("{} fixture(s) failed schema validation", errors);
    }

    // When running default conform (no --file / --fixtures), also check fixture mirror
    if is_default_run {
        check_fixture_mirror()?;
    }

    Ok(())
}

fn collect_json_files(dir: &Path, prefix: Option<&str>) -> anyhow::Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if let Some(required_prefix) = prefix
            && !name.starts_with(required_prefix)
        {
            continue;
        }

        files.push(path);
    }

    Ok(files)
}

fn cmd_sync_fixtures() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");

    sync_fixtures(&golden_dir, &contracts_dir)?;
    Ok(())
}

fn sync_fixtures(golden_dir: &Path, contracts_dir: &Path) -> anyhow::Result<u32> {
    fs::create_dir_all(contracts_dir)
        .with_context(|| format!("create dir {}", contracts_dir.display()))?;

    let mut count = 0u32;
    for entry in
        fs::read_dir(golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let dest = contracts_dir.join(path.file_name().unwrap());
            fs::copy(&path, &dest)
                .with_context(|| format!("copy {} -> {}", path.display(), dest.display()))?;
            println!("  synced  {}", dest.display());
            count += 1;
        }
    }

    println!("\nSynced {} fixtures from golden -> contracts", count);
    Ok(count)
}

/// Check that golden fixtures and contract fixtures are byte-for-byte identical.
fn check_fixture_mirror() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");
    check_fixture_mirror_at(&golden_dir, &contracts_dir)
}

fn check_fixture_mirror_at(golden_dir: &Path, contracts_dir: &Path) -> anyhow::Result<()> {
    if !contracts_dir.is_dir() {
        anyhow::bail!(
            "{} does not exist. Run: cargo run -p xtask -- sync-fixtures",
            contracts_dir.display()
        );
    }

    let mut drift = 0u32;
    for entry in
        fs::read_dir(golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let contract_path = contracts_dir.join(path.file_name().unwrap());
            if !contract_path.exists() {
                println!(
                    "  DRIFT  {} missing in contracts/fixtures/",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
                continue;
            }

            let golden_bytes = fs::read(&path)?;
            let contract_bytes = fs::read(&contract_path)?;
            if golden_bytes != contract_bytes {
                println!(
                    "  DRIFT  {} differs between golden and contracts",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
            }
        }
    }

    // Check for extra files in contracts/fixtures/ (contract -> golden)
    for entry in fs::read_dir(contracts_dir)
        .with_context(|| format!("read dir {}", contracts_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let golden_path = golden_dir.join(path.file_name().unwrap());
            if !golden_path.exists() {
                println!(
                    "  DRIFT  {} unexpected in contracts/fixtures/ (not in golden)",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
            }
        }
    }

    if drift > 0 {
        anyhow::bail!(
            "{} fixture(s) drifted. Run: cargo run -p xtask -- sync-fixtures",
            drift
        );
    }

    println!("  OK  golden and contracts fixtures are in sync");
    Ok(())
}

fn cmd_mutants(
    crate_name: Option<MutantsCrate>,
    summary: bool,
    args: Vec<String>,
) -> anyhow::Result<()> {
    // Typical usage: `cargo install cargo-mutants` then `cargo run -p xtask -- mutants`.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = std::process::Command::new(cargo);
    cmd.arg("mutants");

    // Add --package flag if a specific crate is requested
    if let Some(krate) = crate_name {
        cmd.arg("--package").arg(krate.as_package_name());
    }

    // Forward any extra args
    for a in args {
        cmd.arg(a);
    }

    let status = cmd.status().context("running cargo mutants")?;

    // Generate summary report if requested, regardless of exit status
    // cargo-mutants exits 2 for missed mutants, 3 for timeouts - we still want the summary
    if summary {
        generate_mutation_summary(crate_name)?;
    }

    // Propagate cargo-mutants exit code
    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}

/// Generate a summary report of mutation testing results
fn generate_mutation_summary(crate_name: Option<MutantsCrate>) -> anyhow::Result<()> {
    let outcomes_path = PathBuf::from("mutants.out/outcomes.json");

    if !outcomes_path.exists() {
        println!("\n⚠️  No mutation testing results found at mutants.out/outcomes.json");
        println!("   Run mutation testing first to generate results.");
        return Ok(());
    }

    let outcomes_content =
        fs::read_to_string(&outcomes_path).context("reading mutation outcomes")?;
    let outcomes: serde_json::Value =
        serde_json::from_str(&outcomes_content).context("parsing mutation outcomes")?;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              MUTATION TESTING SUMMARY REPORT                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if let Some(krate) = crate_name {
        println!("Crate: {}", krate.as_package_name());
        println!("Target kill rate: {}%\n", krate.target_kill_rate());
    } else {
        println!("Scope: All workspace crates\n");
        println!("Target kill rates by crate:");
        println!("  • perfgate-domain:   100%");
        println!("  • perfgate-types:     95%");
        println!("  • perfgate-app:       90%");
        println!("  • perfgate-adapters:  80%");
        println!("  • perfgate-cli:       70%\n");
    }

    // Parse outcomes and count results
    let mut killed = 0u32;
    let mut survived = 0u32;
    let mut timeout = 0u32;
    let mut unviable = 0u32;

    if let Some(outcomes_array) = outcomes.as_array() {
        for outcome in outcomes_array {
            if let Some(summary) = outcome.get("summary").and_then(|s| s.as_str()) {
                // cargo-mutants uses: CaughtMutant, MissedMutant, Timeout, Unviable
                match summary {
                    "CaughtMutant" => killed += 1,
                    "MissedMutant" => survived += 1,
                    "Timeout" => timeout += 1,
                    "Unviable" => unviable += 1,
                    _ => {}
                }
            }
        }
    }

    let total = killed + survived + timeout;
    let kill_rate = if total > 0 {
        (killed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Results                                                     │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  ✓ Killed:    {:>5}                                        │",
        killed
    );
    println!(
        "│  ✗ Survived:  {:>5}                                        │",
        survived
    );
    println!(
        "│  ⏱ Timeout:   {:>5}                                        │",
        timeout
    );
    println!(
        "│  ⊘ Unviable:  {:>5}                                        │",
        unviable
    );
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  Total:       {:>5}                                        │",
        total
    );
    println!(
        "│  Kill Rate:   {:>5.1}%                                       │",
        kill_rate
    );
    println!("└─────────────────────────────────────────────────────────────┘");

    // Check against target if a specific crate was tested
    if let Some(krate) = crate_name {
        let target = krate.target_kill_rate() as f64;
        println!();
        if kill_rate >= target {
            println!(
                "✅ Kill rate meets target ({:.1}% >= {}%)",
                kill_rate, target as u8
            );
        } else {
            println!(
                "❌ Kill rate below target ({:.1}% < {}%)",
                kill_rate, target as u8
            );
            println!("\n   Consider adding tests to kill surviving mutants.");
            println!("   Check mutants.out/caught.txt and mutants.out/missed.txt for details.");
        }
    }

    // List surviving mutants if any
    if survived > 0 {
        let missed_path = PathBuf::from("mutants.out/missed.txt");
        if missed_path.exists() {
            println!("\n┌─────────────────────────────────────────────────────────────┐");
            println!("│ Surviving Mutants (tests needed)                            │");
            println!("└─────────────────────────────────────────────────────────────┘");
            let missed_content = fs::read_to_string(&missed_path).unwrap_or_default();
            for (i, line) in missed_content.lines().take(10).enumerate() {
                println!("  {}. {}", i + 1, line);
            }
            if missed_content.lines().count() > 10 {
                println!(
                    "  ... and {} more (see mutants.out/missed.txt)",
                    missed_content.lines().count() - 10
                );
            }
        }
    }

    println!();
    Ok(())
}

fn run<const N: usize>(bin: &str, args: [&str; N]) -> anyhow::Result<()> {
    let status = std::process::Command::new(bin)
        .args(args)
        .status()
        .with_context(|| format!("running {bin}"))?;
    if !status.success() {
        anyhow::bail!("{bin} failed: {status}");
    }
    Ok(())
}

fn run_with_env<const N: usize>(
    bin: &str,
    args: [&str; N],
    envs: &[(&str, &str)],
) -> anyhow::Result<()> {
    if envs.is_empty() {
        return run(bin, args);
    }

    let mut command = std::process::Command::new(bin);
    command.args(args);
    for &(k, v) in envs {
        command.env(k, v);
    }
    let status = command.status().with_context(|| format!("running {bin}"))?;
    if !status.success() {
        anyhow::bail!("{bin} failed: {status}");
    }
    Ok(())
}

fn cmd_schema(out_dir: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(out_dir).with_context(|| format!("create dir {}", out_dir.display()))?;

    write_schema(
        out_dir,
        SCHEMA_FILES[0],
        schema_for!(perfgate_types::RunReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[1],
        schema_for!(perfgate_types::CompareReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[2],
        schema_for!(perfgate_types::ConfigFile),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[3],
        schema_for!(perfgate_types::PerfgateReport),
    )?;

    // Sensor report schema is vendored from contracts/, not generated.
    let vendored_schema = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let dest = out_dir.join(SCHEMA_FILES[4]);
    fs::copy(&vendored_schema, &dest).with_context(|| {
        format!(
            "copy vendored schema {} -> {}",
            vendored_schema.display(),
            dest.display()
        )
    })?;

    Ok(())
}

fn cmd_schema_check(schemas_dir: &Path) -> anyhow::Result<()> {
    if !schemas_dir.exists() {
        anyhow::bail!(
            "{} does not exist. Run: cargo run -p xtask -- schema",
            schemas_dir.display()
        );
    }
    if !schemas_dir.is_dir() {
        anyhow::bail!(
            "{} is not a directory. Run: cargo run -p xtask -- schema",
            schemas_dir.display()
        );
    }

    let generated_dir = xtask::unique_temp_dir("perfgate_schema_check");
    let result = (|| -> anyhow::Result<()> {
        cmd_schema(&generated_dir)?;
        check_schema_mirror_at(&generated_dir, schemas_dir)
    })();

    let _ = fs::remove_dir_all(&generated_dir);
    result
}

fn check_schema_mirror_at(generated_dir: &Path, committed_dir: &Path) -> anyhow::Result<()> {
    let mut drift = 0u32;

    for name in SCHEMA_FILES {
        let generated_path = generated_dir.join(name);
        let committed_path = committed_dir.join(name);

        if !committed_path.exists() {
            println!("  DRIFT  {} missing in {}", name, committed_dir.display());
            drift += 1;
            continue;
        }

        let generated_bytes = fs::read(&generated_path)
            .with_context(|| format!("read {}", generated_path.display()))?;
        let committed_bytes = fs::read(&committed_path)
            .with_context(|| format!("read {}", committed_path.display()))?;
        if generated_bytes != committed_bytes {
            println!("  DRIFT  {} differs from generated schema", name);
            drift += 1;
        }
    }

    for entry in fs::read_dir(committed_dir)
        .with_context(|| format!("read dir {}", committed_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if !SCHEMA_FILES.contains(&name) {
            println!(
                "  DRIFT  {} unexpected in {}",
                name,
                committed_dir.display()
            );
            drift += 1;
        }
    }

    if drift > 0 {
        anyhow::bail!(
            "{} schema file(s) drifted. Run: cargo run -p xtask -- schema",
            drift
        );
    }

    println!("  OK  schema files are locked and up to date");
    Ok(())
}

fn write_schema<T: serde::Serialize>(
    out_dir: &std::path::Path,
    name: &str,
    schema: T,
) -> anyhow::Result<()> {
    let path = out_dir.join(name);
    let json = serde_json::to_vec_pretty(&schema)?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// List all microcrates and their purposes.
fn cmd_microcrates() -> anyhow::Result<()> {
    println!("Perfgate Microcrates");
    println!("===================\n");

    let microcrates = [
        (
            "perfgate-error",
            "Unified error types for error propagation",
            100,
        ),
        (
            "perfgate-sha256",
            "Minimal SHA-256 implementation (no_std compatible)",
            100,
        ),
        (
            "perfgate-stats",
            "Statistical functions (median, percentile, variance)",
            100,
        ),
        ("perfgate-validation", "Bench name validation logic", 100),
        (
            "perfgate-host-detect",
            "Host mismatch detection for CI noise reduction",
            100,
        ),
        (
            "perfgate-budget",
            "Budget evaluation logic for performance thresholds",
            100,
        ),
        (
            "perfgate-significance",
            "Statistical significance testing (Welch's t-test)",
            100,
        ),
        (
            "perfgate-export",
            "Export formats (CSV, JSONL, HTML, Prometheus)",
            90,
        ),
        (
            "perfgate-render",
            "Markdown and GitHub annotations rendering",
            90,
        ),
        (
            "perfgate-sensor",
            "Sensor report builder for cockpit integration",
            90,
        ),
        (
            "perfgate-paired",
            "Paired benchmarking statistics (A/B testing)",
            100,
        ),
        (
            "perfgate-fake",
            "Test utilities and fake implementations",
            70,
        ),
    ];

    println!("{:<25} {:<55} {:>10}", "Crate", "Description", "Kill Rate");
    println!("{:-<25} {:-<55} {:->10}", "", "", "");

    for (name, desc, rate) in &microcrates {
        println!("{:<25} {:<55} {:>9}%", name, desc, rate);
    }

    println!("\nCore Crates");
    println!("-----------\n");

    let core_crates = [
        (
            "perfgate-types",
            "Receipt/config structs, JSON schema types",
            95,
        ),
        ("perfgate-domain", "Pure math/policy (I/O-free)", 100),
        (
            "perfgate-adapters",
            "Platform I/O (process execution, host probing)",
            80,
        ),
        (
            "perfgate-app",
            "Use-cases, rendering, sensor report builder",
            90,
        ),
        (
            "perfgate-cli",
            "CLI argument parsing and command dispatch",
            70,
        ),
    ];

    println!("{:<25} {:<55} {:>10}", "Crate", "Description", "Kill Rate");
    println!("{:-<25} {:-<55} {:->10}", "", "", "");

    for (name, desc, rate) in &core_crates {
        println!("{:<25} {:<55} {:>9}%", name, desc, rate);
    }

    println!("\nDependency Flow");
    println!("--------------\n");
    println!("  perfgate-error (innermost - unified errors)");
    println!("         ↓");
    println!("  perfgate-sha256 (standalone, no_std)");
    println!("         ↓");
    println!("  perfgate-stats (pure math)");
    println!("         ↓");
    println!("  perfgate-validation, perfgate-host-detect (pure logic)");
    println!("         ↓");
    println!("  perfgate-types (data contracts)");
    println!("         ↓");
    println!("  perfgate-budget, perfgate-significance");
    println!("         ↓");
    println!("  perfgate-export, perfgate-render, perfgate-sensor, perfgate-paired");
    println!("         ↓");
    println!("  perfgate-domain (policy)");
    println!("         ↓");
    println!("  perfgate-adapters (platform I/O)");
    println!("         ↓");
    println!("  perfgate-app (use cases)");
    println!("         ↓");
    println!("  perfgate-cli (entry point)");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtask::*;

    #[test]
    fn mutants_crate_mapping_and_targets() {
        assert_eq!(MutantsCrate::Domain.as_package_name(), "perfgate-domain");
        assert_eq!(MutantsCrate::Types.as_package_name(), "perfgate-types");
        assert_eq!(MutantsCrate::App.as_package_name(), "perfgate-app");
        assert_eq!(
            MutantsCrate::Adapters.as_package_name(),
            "perfgate-adapters"
        );
        assert_eq!(MutantsCrate::Cli.as_package_name(), "perfgate-cli");

        assert_eq!(MutantsCrate::Domain.target_kill_rate(), 100);
        assert_eq!(MutantsCrate::Types.target_kill_rate(), 95);
        assert_eq!(MutantsCrate::App.target_kill_rate(), 90);
        assert_eq!(MutantsCrate::Adapters.target_kill_rate(), 80);
        assert_eq!(MutantsCrate::Cli.target_kill_rate(), 70);
    }

    #[test]
    fn run_reports_failure_and_success() {
        #[cfg(windows)]
        {
            assert!(run("cmd", ["/c", "exit", "1"]).is_err());
            assert!(run("cmd", ["/c", "exit", "0"]).is_ok());
        }

        #[cfg(unix)]
        {
            assert!(run("sh", ["-c", "exit 1"]).is_err());
            assert!(run("sh", ["-c", "exit 0"]).is_ok());
        }
    }

    #[test]
    fn cmd_schema_writes_expected_files() {
        let out_dir = unique_temp_dir("perfgate_schema");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
        });

        for name in SCHEMA_FILES {
            let path = out_dir.join(name);
            assert!(path.exists(), "expected schema file {}", name);
            let bytes = fs::read(&path).expect("read schema");
            assert!(
                !bytes.is_empty(),
                "schema file {} should not be empty",
                name
            );
        }

        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_conform_accepts_valid_single_file() {
        with_repo_cwd(|| {
            let path = PathBuf::from("contracts/fixtures/sensor_report_pass.json");
            cmd_conform(None, Some(path)).expect("conform should succeed");
        });
    }

    #[test]
    fn cmd_conform_rejects_invalid_file() {
        let temp_dir = unique_temp_dir("perfgate_invalid_fixture");
        let bad_path = temp_dir.join("bad.json");
        fs::write(&bad_path, r#"{"schema":"sensor.report.v1"}"#).expect("write bad file");
        with_repo_cwd(|| {
            let result = cmd_conform(None, Some(bad_path.clone()));
            assert!(result.is_err(), "expected schema validation to fail");
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cmd_conform_accepts_fixtures_dir_without_sensor_prefix() {
        let temp_dir = unique_temp_dir("perfgate_fixtures_generic");
        with_repo_cwd(|| {
            let valid = fs::read_to_string("contracts/fixtures/sensor_report_pass.json")
                .expect("read canonical fixture");
            fs::write(temp_dir.join("third_party_report.json"), valid).expect("write fixture");

            cmd_conform(Some(temp_dir.clone()), None).expect("fixtures dir should validate");
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cmd_conform_rejects_invalid_generic_json_in_fixtures_dir() {
        let temp_dir = unique_temp_dir("perfgate_fixtures_invalid");
        with_repo_cwd(|| {
            fs::write(
                temp_dir.join("third_party_bad.json"),
                r#"{"schema":"sensor.report.v1"}"#,
            )
            .expect("write bad fixture");

            let err = cmd_conform(Some(temp_dir.clone()), None).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("failed schema validation"),
                "unexpected: {}",
                msg
            );
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn mutation_summary_no_results_is_ok() {
        with_temp_cwd(|_dir| {
            let result = generate_mutation_summary(None);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn mutation_summary_parses_outcomes() {
        with_temp_cwd(|dir| {
            let outcomes_dir = dir.join("mutants.out");
            fs::create_dir_all(&outcomes_dir).expect("create mutants.out");
            fs::write(
                outcomes_dir.join("outcomes.json"),
                r#"[{"summary":"CaughtMutant"},{"summary":"MissedMutant"},{"summary":"Timeout"},{"summary":"Unviable"}]"#,
            )
            .expect("write outcomes");
            fs::write(outcomes_dir.join("missed.txt"), "missed-1\nmissed-2\n")
                .expect("write missed");

            let result = generate_mutation_summary(Some(MutantsCrate::Domain));
            assert!(result.is_ok());
        });
    }

    #[test]
    fn sync_fixtures_copies_sensor_reports_only() {
        let root = unique_temp_dir("perfgate_sync");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_a.json"), "a").expect("write a");
        fs::write(golden.join("sensor_report_b.json"), "b").expect("write b");
        fs::write(golden.join("not_sensor.json"), "no").expect("write other");
        fs::write(golden.join("sensor_report.txt"), "no").expect("write txt");

        let count = sync_fixtures(&golden, &contracts).expect("sync fixtures");
        assert_eq!(count, 2);
        assert_eq!(
            fs::read_to_string(contracts.join("sensor_report_a.json")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(contracts.join("sensor_report_b.json")).unwrap(),
            "b"
        );
        assert!(!contracts.join("not_sensor.json").exists());
        assert!(!contracts.join("sensor_report.txt").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_ok_when_matching() {
        let root = unique_temp_dir("perfgate_mirror_ok");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_ok.json"), "same").expect("write golden");
        fs::write(contracts.join("sensor_report_ok.json"), "same").expect("write contracts");

        check_fixture_mirror_at(&golden, &contracts).expect("mirror check ok");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_requires_contracts_dir() {
        let root = unique_temp_dir("perfgate_mirror_missing");
        let golden = root.join("golden");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::write(golden.join("sensor_report_ok.json"), "same").expect("write golden");

        let missing_contracts = root.join("contracts_missing");
        let err = check_fixture_mirror_at(&golden, &missing_contracts).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("does not exist"), "unexpected error: {}", msg);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_reports_missing_and_different() {
        let root = unique_temp_dir("perfgate_mirror_drift");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_missing.json"), "one").expect("write missing");
        fs::write(golden.join("sensor_report_diff.json"), "golden").expect("write golden");
        fs::write(contracts.join("sensor_report_diff.json"), "contracts").expect("write contracts");

        let err = check_fixture_mirror_at(&golden, &contracts).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("fixture(s) drifted"),
            "unexpected error: {}",
            msg
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cmd_schema_check_accepts_matching_schemas() {
        let out_dir = unique_temp_dir("perfgate_schema_check_ok");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            cmd_schema_check(&out_dir).expect("schema check should pass");
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_missing_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_missing");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::remove_file(out_dir.join(SCHEMA_FILES[0])).expect("remove file");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_extra_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_extra");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::write(out_dir.join("unexpected.schema.json"), "{}").expect("write extra");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_different_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_diff");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::write(out_dir.join(SCHEMA_FILES[1]), "{}").expect("rewrite schema");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }
}
