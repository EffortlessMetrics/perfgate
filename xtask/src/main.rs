use anyhow::Context;
use clap::{Parser, Subcommand};
use schemars::schema_for;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repo automation for perfgate")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// (Re)generate JSON Schemas for receipts and config.
    Schema {
        /// Output directory
        #[arg(long, default_value = "schemas")]
        out_dir: PathBuf,
    },

    /// Run the "usual" repo checks (fmt, clippy, test, schema).
    Ci,

    /// Run mutation testing via cargo-mutants (must be installed).
    Mutants {
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
        Command::Mutants { args } => cmd_mutants(args),
    }
}

fn cmd_ci() -> anyhow::Result<()> {
    run("cargo", ["fmt", "--all", "--", "--check"])?;
    run(
        "cargo",
        ["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
    )?;
    run("cargo", ["test", "--all"])?;
    run("cargo", ["run", "-p", "xtask", "--", "schema"])?;
    Ok(())
}

fn cmd_mutants(args: Vec<String>) -> anyhow::Result<()> {
    // Typical usage: `cargo install cargo-mutants` then `cargo run -p xtask -- mutants`.
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("mutants");
    for a in args {
        cmd.arg(a);
    }
    let status = cmd.status().context("running cargo mutants")?;
    if !status.success() {
        anyhow::bail!("cargo mutants failed: {status}");
    }
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

    Ok(())
}

fn write_schema<T: serde::Serialize>(
    out_dir: &PathBuf,
    name: &str,
    schema: T,
) -> anyhow::Result<()> {
    let path = out_dir.join(name);
    let json = serde_json::to_vec_pretty(&schema)?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
