use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use glob::glob;
use regex::Regex;
use schemars::schema_for;
use std::collections::{BTreeMap, BTreeSet};
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

    /// Dogfooding operations.
    Dogfood {
        #[command(subcommand)]
        action: DogfoodAction,
    },

    /// Update generated docs.
    DocsSync,

    /// Fail CI if generated docs differ from committed docs.
    DocsCheck,

    /// Validate CLI examples in documentation against actual --help output.
    DocTest {
        /// Additional markdown files to scan (default: README.md, CLAUDE.md, docs/*.md)
        #[arg(long)]
        files: Vec<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum DogfoodAction {
    /// (Re)generate stable compare/check fixtures from controlled inputs.
    Fixtures,
    /// Validate expected artifact layout and allowed exit behavior.
    Verify {
        /// Directory containing perfgate output artifacts
        #[arg(long, default_value = "artifacts/perfgate")]
        dir: PathBuf,
    },
    /// Turn nightly outputs into refreshed baseline files.
    Promote,
    /// Generate a compact Markdown/JSON summary of drift, noise, and recommendations.
    Summarize {
        /// Directory containing perfgate export trends
        #[arg(long, default_value = "artifacts/trends")]
        dir: PathBuf,
    },
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
        Command::Dogfood { action } => cmd_dogfood(action),
        Command::DocsSync => cmd_docs_sync(),
        Command::DocsCheck => cmd_docs_check(),
        Command::DocTest { files } => cmd_doc_test(files),
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

        let generated_str = fs::read_to_string(&generated_path)
            .with_context(|| format!("read {}", generated_path.display()))?
            .replace("\r\n", "\n");
        let committed_str = fs::read_to_string(&committed_path)
            .with_context(|| format!("read {}", committed_path.display()))?
            .replace("\r\n", "\n");

        if generated_str != committed_str {
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

fn cmd_dogfood(action: DogfoodAction) -> anyhow::Result<()> {
    match action {
        DogfoodAction::Fixtures => {
            println!("Regenerating dogfooding fixtures...");
            // Ensure selfbench is built
            run("cargo", ["build", "--release", "-p", "perfgate-selfbench"])?;

            let selfbench_bin = if cfg!(windows) {
                "./target/release/perfgate-selfbench.exe"
            } else {
                "./target/release/perfgate-selfbench"
            };

            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "run",
                    "--name",
                    "test-bench",
                    "--repeat",
                    "5",
                    "--warmup",
                    "1",
                    "--out",
                    ".ci/fixtures/compare/small-baseline.json",
                    "--",
                    selfbench_bin,
                    "noop",
                ],
                &[],
            )?;
            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "run",
                    "--name",
                    "test-bench",
                    "--repeat",
                    "5",
                    "--warmup",
                    "1",
                    "--out",
                    ".ci/fixtures/compare/small-current.json",
                    "--",
                    selfbench_bin,
                    "noop",
                ],
                &[],
            )?;
            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "compare",
                    "--baseline",
                    ".ci/fixtures/compare/small-baseline.json",
                    "--current",
                    ".ci/fixtures/compare/small-current.json",
                    "--out",
                    ".ci/fixtures/compare/compare-receipt.json",
                ],
                &[],
            )
            .ok(); // Ignore exit code 2 (policy fail) here
            println!("Fixtures regenerated successfully.");
            Ok(())
        }
        DogfoodAction::Verify { dir } => {
            println!("Verifying dogfooding artifacts in {}...", dir.display());
            let required_files = ["report.json", "comment.md"];
            for file in &required_files {
                let path = dir.join(file);
                if !path.exists() {
                    anyhow::bail!("Missing required artifact: {}", path.display());
                }
                println!("  OK  {}", path.display());
            }

            // Also check extras
            let pattern = format!("{}/extras/**/perfgate.run.v1.json", dir.display());
            let mut count = 0;
            for entry in glob(&pattern)? {
                let path = entry?;
                println!("  OK  {}", path.display());
                count += 1;
            }
            if count == 0 {
                anyhow::bail!("No native receipts found matching {}", pattern);
            }
            println!("Verified {} native receipts.", count);
            Ok(())
        }
        DogfoodAction::Promote => {
            println!("Promoting nightly outputs to baselines...");
            let target_root = Path::new("baselines/gha-ubuntu-24.04-x86_64/");
            fs::create_dir_all(target_root)?;

            let pattern = "artifacts/perfgate/extras/**/perfgate.run.v1.json";
            let mut count = 0;
            for entry in glob(pattern)? {
                let src = entry?;
                let rel = src
                    .strip_prefix("artifacts/perfgate/extras/")
                    .context("invalid path")?;
                let bench_path = rel.parent().context("no bench parent")?;
                let bench_name = bench_path.to_str().context("non-utf8 bench name")?;

                let dest = target_root.join(format!("{}.json", bench_name));
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }

                println!("  Promoting {} -> {}", bench_name, dest.display());

                run_with_env(
                    "cargo",
                    [
                        "run",
                        "--release",
                        "-p",
                        "perfgate-cli",
                        "--bin",
                        "perfgate",
                        "--",
                        "promote",
                        "--current",
                        src.to_str().context("src path")?,
                        "--to",
                        dest.to_str().context("dest path")?,
                        "--normalize",
                    ],
                    &[],
                )?;
                count += 1;
            }
            println!("Promoted {} baselines.", count);
            Ok(())
        }
        DogfoodAction::Summarize { dir } => {
            println!("Generating trend variance summary...");
            let pattern = format!("{}/**/*.jsonl", dir.display());

            let mut all_rows: Vec<perfgate_export::RunExportRow> = Vec::new();
            for entry in glob(&pattern)? {
                let path = entry?;
                let content = fs::read_to_string(&path)?;
                for line in content.lines() {
                    if let Ok(row) = serde_json::from_str::<perfgate_export::RunExportRow>(line) {
                        all_rows.push(row);
                    }
                }
            }

            if all_rows.is_empty() {
                println!("No trend data found in {}", dir.display());
                return Ok(());
            }

            // Group by bench name
            let mut by_bench: std::collections::BTreeMap<String, Vec<f64>> =
                std::collections::BTreeMap::new();
            for row in all_rows {
                by_bench
                    .entry(row.bench_name)
                    .or_default()
                    .push(row.wall_ms_median as f64);
            }

            println!("\n## Weekly Variance Summary");
            println!("\n| Benchmark | Samples | Mean (ms) | StdDev | CV (%) | Rec. Threshold |");
            println!("|-----------|---------|-----------|--------|--------|----------------|");

            for (bench, mut vals) in by_bench {
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let count = vals.len();
                if count < 2 {
                    let mean = vals.first().unwrap_or(&0.0);
                    println!("| {} | {} | {:.2} | N/A | N/A | N/A |", bench, count, mean);
                    continue;
                }

                let (mean, variance) =
                    perfgate_stats::mean_and_variance(&vals).unwrap_or((0.0, 0.0));
                let stddev = variance.sqrt();
                let cv = if mean > 0.0 {
                    (stddev / mean) * 100.0
                } else {
                    0.0
                };

                // Recommended Threshold: usually 3x CV + small buffer
                let rec_thresh = (cv * 3.0).max(5.0); // minimum 5% threshold

                println!(
                    "| {} | {} | {:.2} | {:.2} | {:.2}% | {:.1}% |",
                    bench, count, mean, stddev, cv, rec_thresh
                );
            }

            Ok(())
        }
    }
}

fn cmd_docs_sync() -> anyhow::Result<()> {
    let md = generate_workspace_inventory_md();
    let path = Path::new("docs/WORKSPACE.md");
    fs::write(path, md).with_context(|| format!("write {}", path.display()))?;
    println!("  OK  {}", path.display());

    Ok(())
}

fn generate_workspace_inventory_md() -> String {
    let mut md = String::new();
    md.push_str("# Perfgate Workspace Inventory\n\n");
    md.push_str("This file is automatically generated by `cargo run -p xtask -- docs-sync`.\n\n");

    md.push_str("## Micro-crates\n\n");
    md.push_str("| Crate | Description | Kill Rate Target |\n");
    md.push_str("|-------|-------------|------------------|\n");

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

    for (name, desc, rate) in &microcrates {
        md.push_str(&format!("| `{}` | {} | {}% |\n", name, desc, rate));
    }

    md.push_str("\n## Core Crates\n\n");
    md.push_str("| Crate | Description | Kill Rate Target |\n");
    md.push_str("|-------|-------------|------------------|\n");

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
        ("perfgate-app", "Use-cases, orchestration layer", 90),
        (
            "perfgate-cli",
            "CLI argument parsing and command dispatch",
            70,
        ),
        (
            "perfgate-server",
            "REST API server for baseline management",
            90,
        ),
        (
            "perfgate-client",
            "API client for baseline server interaction",
            90,
        ),
        ("perfgate", "Unified facade library", 90),
    ];

    for (name, desc, rate) in &core_crates {
        md.push_str(&format!("| `{}` | {} | {}% |\n", name, desc, rate));
    }

    md.push_str("\n## Dependency Flow\n\n");
    md.push_str("```mermaid\ngraph TD\n");
    md.push_str("  error[perfgate-error] --> types[perfgate-types]\n");
    md.push_str("  sha[perfgate-sha256] --> types\n");
    md.push_str("  stats[perfgate-stats] --> types\n");
    md.push_str("  val[perfgate-validation] --> types\n");
    md.push_str("  host[perfgate-host-detect] --> types\n");
    md.push_str("  types --> budget[perfgate-budget]\n");
    md.push_str("  types --> sig[perfgate-significance]\n");
    md.push_str("  budget --> domain[perfgate-domain]\n");
    md.push_str("  sig --> domain\n");
    md.push_str("  domain --> adapters[perfgate-adapters]\n");
    md.push_str("  adapters --> app[perfgate-app]\n");
    md.push_str("  app --> cli[perfgate-cli]\n");
    md.push_str("  app --> facade[perfgate]\n");
    md.push_str("  types --> client[perfgate-client]\n");
    md.push_str("  client --> app\n");
    md.push_str("  types --> server[perfgate-server]\n");
    md.push_str("```\n");
    md
}

fn cmd_docs_check() -> anyhow::Result<()> {
    println!("Checking documentation drift...");
    let path = Path::new("docs/WORKSPACE.md");
    if !path.exists() {
        anyhow::bail!(
            "Missing documentation: {}. Run: cargo run -p xtask -- docs-sync",
            path.display()
        );
    }

    let committed = fs::read_to_string(path)?;
    let generated = generate_workspace_inventory_md();

    if committed.replace("\r\n", "\n") != generated.replace("\r\n", "\n") {
        anyhow::bail!(
            "Documentation drift detected in {}. Run: cargo run -p xtask -- docs-sync",
            path.display()
        );
    }

    println!("  OK  documentation is up to date");
    Ok(())
}

// ---------------------------------------------------------------------------
// doc-test: validate CLI examples in documentation
// ---------------------------------------------------------------------------

/// A CLI invocation extracted from a documentation file.
#[derive(Debug, Clone)]
struct DocCommand {
    /// Source file
    file: PathBuf,
    /// Line number (1-based) where the command was found
    line: usize,
    /// The raw command text
    raw: String,
    /// Subcommand path (e.g. ["check"] or ["baseline", "list"])
    subcommand: Vec<String>,
    /// Flags used (e.g. ["--config", "--bench", "--mode"])
    flags: Vec<String>,
}

/// Collect default doc files: README.md, CLAUDE.md, and docs/**/*.md
fn default_doc_files() -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for name in ["README.md", "CLAUDE.md", "CONTRIBUTING.md"] {
        let p = PathBuf::from(name);
        if p.exists() {
            files.push(p);
        }
    }

    for entry in glob("docs/**/*.md")? {
        files.push(entry?);
    }

    files.sort();
    Ok(files)
}

/// Extract perfgate CLI invocations from markdown fenced code blocks.
///
/// Handles both direct `perfgate <subcommand>` and
/// `cargo run -p perfgate-cli -- <subcommand>` patterns.
/// Multi-line commands joined with trailing backslash are supported.
fn extract_commands(file: &Path, content: &str) -> Vec<DocCommand> {
    let mut commands = Vec::new();
    let mut in_code_block = false;
    // Accumulated lines for multi-line commands (trailing backslash)
    let mut continuation: Option<(usize, String)> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        if line.trim_start().starts_with("```") {
            if in_code_block {
                // Closing fence -- flush any pending continuation
                if let Some((start, acc)) = continuation.take()
                    && let Some(cmd) = parse_perfgate_line(file, start, &acc)
                {
                    commands.push(cmd);
                }
            }
            in_code_block = !in_code_block;
            continue;
        }

        if !in_code_block {
            continue;
        }

        let trimmed = line.trim();

        // Handle line continuation (trailing backslash)
        if let Some((start, ref mut acc)) = continuation {
            // Append this line (strip leading whitespace, it's a continuation)
            acc.push(' ');
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                acc.push_str(stripped.trim());
            } else {
                acc.push_str(trimmed);
                // End of continuation -- parse accumulated line
                let full = std::mem::take(acc);
                let start_line = start;
                continuation = None;
                if let Some(cmd) = parse_perfgate_line(file, start_line, &full) {
                    commands.push(cmd);
                }
            }
            continue;
        }

        // New line -- check for trailing backslash
        if let Some(stripped) = trimmed.strip_suffix('\\') {
            // Start a continuation
            continuation = Some((line_num, stripped.trim().to_string()));
            continue;
        }

        if let Some(cmd) = parse_perfgate_line(file, line_num, trimmed) {
            commands.push(cmd);
        }
    }

    // Flush any trailing continuation (shouldn't normally happen)
    if let Some((start, acc)) = continuation
        && let Some(cmd) = parse_perfgate_line(file, start, &acc)
    {
        commands.push(cmd);
    }

    commands
}

/// Try to parse a single line as a perfgate CLI invocation.
fn parse_perfgate_line(file: &Path, line: usize, text: &str) -> Option<DocCommand> {
    // Strip shell prefixes like `$ ` or `> `
    let text = text
        .strip_prefix("$ ")
        .or_else(|| text.strip_prefix("> "))
        .unwrap_or(text);

    // Match `perfgate <args>` or `cargo run -p perfgate-cli [--bin perfgate] -- <args>`
    let args_str = if let Some(rest) = strip_cargo_run_prefix(text) {
        rest
    } else if let Some(rest) = text.strip_prefix("perfgate ") {
        rest
    } else if text == "perfgate" {
        ""
    } else {
        return None;
    };

    let tokens = shell_tokenize(args_str);
    if tokens.is_empty() {
        return None;
    }

    // Extract subcommand path and flags
    let mut subcommand = Vec::new();
    let mut flags = Vec::new();

    for token in &tokens {
        if token == "--" {
            // End-of-flags separator: everything after is passed to the sub-process
            break;
        }
        if token.starts_with('-') {
            // It's a flag -- extract just the flag name (e.g. "--config" from "--config=foo")
            let flag = token.split('=').next().unwrap_or(token).to_string();
            flags.push(flag);
        } else if flags.is_empty() {
            // Before any flags, it's part of the subcommand path
            subcommand.push(token.clone());
        }
        // After the first flag, positional args are arguments, not subcommands
    }

    // Skip if no subcommand found
    if subcommand.is_empty() {
        return None;
    }

    // The subcommand path might include positional arguments after the actual subcommand.
    // We'll validate against --help to determine which are real subcommands.

    Some(DocCommand {
        file: file.to_path_buf(),
        line,
        raw: text.to_string(),
        subcommand,
        flags,
    })
}

/// Strip the `cargo run -p perfgate-cli [--bin perfgate] [--release] -- ` prefix.
fn strip_cargo_run_prefix(text: &str) -> Option<&str> {
    // Look for `cargo run -p perfgate-cli` followed by optional flags and then `--`
    let re = Regex::new(
        r"^cargo\s+run\s+(?:--release\s+)?-p\s+perfgate-cli(?:\s+--bin\s+perfgate)?(?:\s+--release)?\s+--\s+",
    )
    .ok()?;

    re.find(text).map(|m| &text[m.end()..])
}

/// Simple shell tokenizer: splits on whitespace, respects double quotes.
fn shell_tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in s.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Get the list of valid subcommands from `perfgate --help` output.
fn get_valid_subcommands(cargo: &str) -> anyhow::Result<BTreeSet<String>> {
    let output = std::process::Command::new(cargo)
        .args(["run", "-p", "perfgate-cli", "--", "--help"])
        .output()
        .context("running perfgate --help")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_subcommands_from_help(&stdout)
}

/// Parse subcommand names from --help output.
fn parse_subcommands_from_help(help_text: &str) -> anyhow::Result<BTreeSet<String>> {
    let mut subcommands = BTreeSet::new();
    let mut in_commands_section = false;

    for line in help_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Commands:") {
            in_commands_section = true;
            continue;
        }
        if !in_commands_section {
            continue;
        }

        // Blank lines within the section are fine -- skip them
        if trimmed.is_empty() {
            continue;
        }

        // A non-indented, non-empty line means we've left the Commands section
        if !line.starts_with(' ') {
            in_commands_section = false;
            continue;
        }

        // Parse "  subcommand   description"
        if let Some(name) = trimmed.split_whitespace().next()
            && name != "help"
        {
            subcommands.insert(name.to_string());
        }
    }

    Ok(subcommands)
}

/// Get valid flags for a specific subcommand by running `perfgate <subcmd> --help`.
fn get_valid_flags(cargo: &str, subcmd: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut args = vec!["run", "-p", "perfgate-cli", "--"];
    let subcmd_strs: Vec<&str> = subcmd.iter().map(|s| s.as_str()).collect();
    args.extend_from_slice(&subcmd_strs);
    args.push("--help");

    let output = std::process::Command::new(cargo)
        .args(&args)
        .output()
        .with_context(|| format!("running perfgate {} --help", subcmd.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    parse_flags_from_help(&combined)
}

/// Parse flag names from --help output.
fn parse_flags_from_help(help_text: &str) -> anyhow::Result<BTreeSet<String>> {
    let mut flags = BTreeSet::new();
    let re = Regex::new(r"--[a-zA-Z][a-zA-Z0-9_-]*").context("compile flag regex")?;

    for mat in re.find_iter(help_text) {
        flags.insert(mat.as_str().to_string());
    }

    // Always include --help and --version as universally valid
    flags.insert("--help".to_string());
    flags.insert("--version".to_string());

    Ok(flags)
}

fn cmd_doc_test(extra_files: Vec<PathBuf>) -> anyhow::Result<()> {
    println!("Validating CLI examples in documentation...\n");

    // Collect files to scan
    let mut files = default_doc_files()?;
    files.extend(extra_files);
    files.sort();
    files.dedup();

    if files.is_empty() {
        anyhow::bail!("no documentation files found");
    }

    // Extract all commands from all files
    let mut all_commands = Vec::new();
    for file in &files {
        let content =
            fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let cmds = extract_commands(file, &content);
        all_commands.extend(cmds);
    }

    println!(
        "Found {} CLI examples in {} files\n",
        all_commands.len(),
        files.len()
    );

    if all_commands.is_empty() {
        println!("No perfgate CLI examples found in documentation.");
        return Ok(());
    }

    // Get valid subcommands from the binary
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let valid_subcommands = get_valid_subcommands(&cargo)?;

    println!(
        "Valid subcommands: {}\n",
        valid_subcommands
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    // For each unique subcommand, get valid flags (caching)
    let mut flag_cache: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0u32;

    for cmd in &all_commands {
        checked += 1;
        let first_subcmd = &cmd.subcommand[0];

        // Check if the top-level subcommand is valid
        if !valid_subcommands.contains(first_subcmd) {
            errors.push(format!(
                "  {}:{}: unknown subcommand '{}'\n    {}",
                cmd.file.display(),
                cmd.line,
                first_subcmd,
                cmd.raw
            ));
            continue;
        }

        // Determine the effective subcommand path for --help.
        // For subcommands like "baseline list", we need to pass both words.
        // We try the longest prefix that is a valid sub-subcommand.
        let subcmd_path = resolve_subcommand_path(&cargo, &cmd.subcommand, &mut flag_cache)?;

        // Get valid flags for this subcommand
        let cache_key = subcmd_path.join(" ");
        if !flag_cache.contains_key(&cache_key) {
            let flags = get_valid_flags(&cargo, &subcmd_path)?;
            flag_cache.insert(cache_key.clone(), flags);
        }
        let valid_flags = &flag_cache[&cache_key];

        // Check each flag
        for flag in &cmd.flags {
            if !valid_flags.contains(flag) {
                errors.push(format!(
                    "  {}:{}: unknown flag '{}' for 'perfgate {}'\n    {}",
                    cmd.file.display(),
                    cmd.line,
                    flag,
                    cache_key,
                    cmd.raw
                ));
            }
        }
    }

    println!("Checked {} examples", checked);

    if errors.is_empty() {
        println!("\n  OK  all CLI examples are valid");
        Ok(())
    } else {
        println!("\nFound {} error(s):\n", errors.len());
        for err in &errors {
            println!("{}\n", err);
        }
        anyhow::bail!(
            "{} documentation example(s) have invalid subcommands or flags",
            errors.len()
        );
    }
}

/// Resolve the subcommand path (e.g., ["baseline", "list"]) by checking
/// if deeper subcommands are valid.
fn resolve_subcommand_path(
    cargo: &str,
    tokens: &[String],
    cache: &mut BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<Vec<String>> {
    if tokens.len() <= 1 {
        return Ok(tokens.to_vec());
    }

    // Try the first two tokens as a subcommand path (e.g. "baseline list")
    let two_deep = &tokens[..2];
    let cache_key = two_deep.join(" ");

    if let std::collections::btree_map::Entry::Vacant(e) = cache.entry(cache_key) {
        // Try getting help for the two-level subcommand
        if let Ok(flags) = get_valid_flags(cargo, two_deep)
            && !flags.is_empty()
        {
            e.insert(flags);
            return Ok(two_deep.to_vec());
        }
    } else {
        return Ok(two_deep.to_vec());
    }

    // Fall back to just the first token
    Ok(vec![tokens[0].clone()])
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

    // --- doc-test unit tests ---

    #[test]
    fn extract_commands_from_bash_block() {
        let md = r#"
# Example

```bash
perfgate run --name bench --out out.json -- echo hello
perfgate compare --baseline base.json --current cur.json
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].subcommand, vec!["run"]);
        assert!(cmds[0].flags.contains(&"--name".to_string()));
        assert!(cmds[0].flags.contains(&"--out".to_string()));
        assert_eq!(cmds[1].subcommand, vec!["compare"]);
        assert!(cmds[1].flags.contains(&"--baseline".to_string()));
        assert!(cmds[1].flags.contains(&"--current".to_string()));
    }

    #[test]
    fn extract_commands_cargo_run_form() {
        let md = r#"
```bash
cargo run -p perfgate-cli -- check --config perfgate.toml --bench my-bench
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["check"]);
        assert!(cmds[0].flags.contains(&"--config".to_string()));
        assert!(cmds[0].flags.contains(&"--bench".to_string()));
    }

    #[test]
    fn extract_commands_multiline_continuation() {
        let md = r#"
```bash
perfgate check --config perfgate.toml \
  --bench my-bench \
  --mode cockpit
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["check"]);
        assert!(cmds[0].flags.contains(&"--config".to_string()));
        assert!(cmds[0].flags.contains(&"--bench".to_string()));
        assert!(cmds[0].flags.contains(&"--mode".to_string()));
    }

    #[test]
    fn extract_commands_ignores_non_perfgate() {
        let md = r#"
```bash
echo hello
cargo build --release
ls -la
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert!(cmds.is_empty());
    }

    #[test]
    fn extract_commands_ignores_outside_code_blocks() {
        let md = r#"
Run `perfgate check --config perfgate.toml` to validate.

perfgate run --name bench -- echo hello
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert!(cmds.is_empty());
    }

    #[test]
    fn extract_commands_baseline_subsubcommand() {
        let md = r#"
```bash
perfgate baseline list --project my-project
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["baseline", "list"]);
        assert!(cmds[0].flags.contains(&"--project".to_string()));
    }

    #[test]
    fn shell_tokenize_basic() {
        let tokens = shell_tokenize("--name bench --out out.json -- echo hello");
        assert_eq!(
            tokens,
            vec![
                "--name", "bench", "--out", "out.json", "--", "echo", "hello"
            ]
        );
    }

    #[test]
    fn shell_tokenize_with_quotes() {
        let tokens = shell_tokenize(r#"--name "my bench" --out out.json"#);
        assert_eq!(tokens, vec!["--name", "my bench", "--out", "out.json"]);
    }

    #[test]
    fn parse_subcommands_from_help_output() {
        let help = r#"Usage: perfgate [OPTIONS] <COMMAND>

Commands:
  run                 Run a command
  compare             Compare receipts
  check               Config-driven workflow
  baseline            Manage baselines
  help                Print help

Options:
  -h, --help     Print help
"#;
        let subs = parse_subcommands_from_help(help).unwrap();
        assert!(subs.contains("run"));
        assert!(subs.contains("compare"));
        assert!(subs.contains("check"));
        assert!(subs.contains("baseline"));
        assert!(!subs.contains("help"));
    }

    #[test]
    fn parse_flags_from_help_output() {
        let help = r#"Usage: perfgate run [OPTIONS] -- <COMMAND>...

Options:
      --name <NAME>      Benchmark name
      --repeat <REPEAT>  Number of repetitions [default: 7]
      --out <OUT>        Output file path
  -h, --help             Print help
"#;
        let flags = parse_flags_from_help(help).unwrap();
        assert!(flags.contains("--name"));
        assert!(flags.contains("--repeat"));
        assert!(flags.contains("--out"));
        assert!(flags.contains("--help"));
    }

    #[test]
    fn strip_cargo_run_prefix_variants() {
        assert_eq!(
            strip_cargo_run_prefix("cargo run -p perfgate-cli -- check --config foo.toml"),
            Some("check --config foo.toml")
        );
        assert_eq!(
            strip_cargo_run_prefix("cargo run -p perfgate-cli --bin perfgate -- run --name bench"),
            Some("run --name bench")
        );
        assert_eq!(
            strip_cargo_run_prefix("cargo run --release -p perfgate-cli -- run --name bench"),
            Some("run --name bench")
        );
        assert_eq!(strip_cargo_run_prefix("echo hello"), None);
    }

    #[test]
    fn extract_commands_with_dollar_prefix() {
        let md = r#"
```bash
$ perfgate run --name bench --out out.json -- echo hello
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["run"]);
    }
}
