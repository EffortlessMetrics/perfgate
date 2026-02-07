use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use schemars::schema_for;
use std::fs;
use std::path::PathBuf;

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
}

impl MutantsCrate {
    fn as_package_name(&self) -> &'static str {
        match self {
            MutantsCrate::Domain => "perfgate-domain",
            MutantsCrate::Types => "perfgate-types",
            MutantsCrate::App => "perfgate-app",
            MutantsCrate::Adapters => "perfgate-adapters",
            MutantsCrate::Cli => "perfgate-cli",
        }
    }

    fn target_kill_rate(&self) -> u8 {
        match self {
            MutantsCrate::Domain => 100,
            MutantsCrate::Types => 95,
            MutantsCrate::App => 90,
            MutantsCrate::Adapters => 80,
            MutantsCrate::Cli => 70,
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

    /// Run the "usual" repo checks (fmt, clippy, test, schema, conform).
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Schema { out_dir } => cmd_schema(&out_dir),
        Command::Ci => cmd_ci(),
        Command::Conform { fixtures, file } => cmd_conform(fixtures, file),
        Command::SyncFixtures => cmd_sync_fixtures(),
        Command::Mutants {
            crate_name,
            summary,
            args,
        } => cmd_mutants(crate_name, summary, args),
    }
}

fn cmd_ci() -> anyhow::Result<()> {
    run("cargo", ["fmt", "--all", "--", "--check"])?;
    run(
        "cargo",
        [
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run("cargo", ["test", "--all"])?;
    run("cargo", ["run", "-p", "xtask", "--", "schema"])?;
    run("cargo", ["run", "-p", "xtask", "--", "conform"])?;
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
    } else {
        // Default: golden fixtures + contracts/fixtures
        let default_dirs = [
            fixtures_dir
                .unwrap_or_else(|| PathBuf::from("crates/perfgate-cli/tests/fixtures/golden")),
            PathBuf::from("contracts/fixtures"),
        ];

        for dir in &default_dirs {
            if dir.is_dir() {
                for entry in
                    fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))?
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
                        files_to_validate.push(path);
                    }
                }
            }
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

fn cmd_sync_fixtures() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");

    fs::create_dir_all(&contracts_dir)
        .with_context(|| format!("create dir {}", contracts_dir.display()))?;

    let mut count = 0u32;
    for entry in
        fs::read_dir(&golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
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
    Ok(())
}

/// Check that golden fixtures and contract fixtures are byte-for-byte identical.
fn check_fixture_mirror() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");

    if !contracts_dir.is_dir() {
        anyhow::bail!(
            "contracts/fixtures/ does not exist. Run: cargo run -p xtask -- sync-fixtures"
        );
    }

    let mut drift = 0u32;
    for entry in
        fs::read_dir(&golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
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
    let mut cmd = std::process::Command::new("cargo");
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

fn cmd_schema(out_dir: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(out_dir).with_context(|| format!("create dir {}", out_dir.display()))?;

    write_schema(
        out_dir,
        "perfgate.run.v1.schema.json",
        schema_for!(perfgate_types::RunReceipt),
    )?;

    write_schema(
        out_dir,
        "perfgate.compare.v1.schema.json",
        schema_for!(perfgate_types::CompareReceipt),
    )?;

    write_schema(
        out_dir,
        "perfgate.config.v1.schema.json",
        schema_for!(perfgate_types::ConfigFile),
    )?;

    write_schema(
        out_dir,
        "perfgate.report.v1.schema.json",
        schema_for!(perfgate_types::PerfgateReport),
    )?;

    // Sensor report schema is vendored from contracts/, not generated.
    let vendored_schema = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let dest = out_dir.join("sensor.report.v1.schema.json");
    fs::copy(&vendored_schema, &dest).with_context(|| {
        format!(
            "copy vendored schema {} -> {}",
            vendored_schema.display(),
            dest.display()
        )
    })?;

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
