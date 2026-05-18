use crate::{
    BenchmarkSuggestionProfile, DEFAULT_FALLBACK_BASELINE_DIR, InitArgs, InitCiPlatform, InitPreset,
};
use anyhow::Context;
use perfgate::app as perfgate_app;
use perfgate_app::baseline_resolve::is_remote_storage_uri;
use perfgate_app::init::{
    CiPlatform, Preset, ci_workflow_path, discover_benchmarks, generate_config, render_config_toml,
    render_onboarding_readme, scaffold_ci,
};
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_benchmark_suggestion_profile(
    requested: BenchmarkSuggestionProfile,
    scan_dir: &Path,
) -> BenchmarkSuggestionProfile {
    if requested != BenchmarkSuggestionProfile::Auto {
        return requested;
    }

    if scan_dir.join("package.json").exists() {
        return BenchmarkSuggestionProfile::Node;
    }

    if scan_dir.join("pyproject.toml").exists()
        || scan_dir.join("setup.py").exists()
        || scan_dir.join("requirements.txt").exists()
    {
        return BenchmarkSuggestionProfile::PythonCommand;
    }

    let cargo_toml = scan_dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml) {
        if content.contains("[workspace]") {
            return BenchmarkSuggestionProfile::RustWorkspace;
        }
        return BenchmarkSuggestionProfile::RustCli;
    }

    BenchmarkSuggestionProfile::GenericCommand
}

fn render_benchmark_suggestions(profile: BenchmarkSuggestionProfile) -> String {
    match profile {
        BenchmarkSuggestionProfile::Auto => {
            render_benchmark_suggestions(BenchmarkSuggestionProfile::GenericCommand)
        }
        BenchmarkSuggestionProfile::RustCli => r#"
# Benchmark recipe: rust-cli-smoke
# Review and edit before committing. These are candidates, not policy.
# Best for: CLI startup, argument parsing, and command wiring smoke.
# Bad for: steady-state throughput, parser hot loops, and compile-heavy checks.
# Expected noise: low to medium on stable hosts.
# Recommended mode: advisory until calibrated, then gate if the workload matters.
# Should block PRs: only after baseline and signal maturity are proven.
# Paired-mode hint: use paired mode if host or startup variance dominates.
#
# Fast first-hour check: low setup cost and useful for smoke gating.
# [[bench]]
# name = "cli-help"
# command = ["cargo", "run", "-q", "--", "--help"]
#
# Heavier check: keep advisory until calibrated.
# [[bench]]
# name = "cli-release-help"
# command = ["cargo", "run", "--release", "--", "--help"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::RustWorkspace => r#"
# Benchmark recipe: rust-workspace-advisory
# Review and edit before committing. These are candidates, not policy.
# Best for: broad workspace health and catching expensive integration paths.
# Bad for: first-hour blocking gates and isolated performance attribution.
# Expected noise: medium to high because compile and test setup can dominate.
# Recommended mode: advisory until calibrated; split into smaller gates later.
# Should block PRs: not until compile/test noise is understood.
# Paired-mode hint: use paired mode when CI runner drift changes the verdict.
#
# Fast first-hour check: choose one small package or command with low setup cost.
# [[bench]]
# name = "workspace-smoke"
# command = ["cargo", "test", "-p", "your-package", "--no-fail-fast"]
#
# Heavier check: compile-heavy workspace tests should stay advisory until calibrated.
# [[bench]]
# name = "workspace-test"
# command = ["cargo", "test", "--workspace", "--no-fail-fast"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::Node => r#"
# Benchmark recipe: node-command
# Review and edit before committing. These are candidates, not policy.
# Best for: dedicated Node benchmark scripts with stable local input.
# Bad for: package install, network calls, and commands that mix build/test/perf.
# Expected noise: low to medium when dependencies and inputs are stable.
# Recommended mode: advisory until calibrated; gate only stable scripts.
# Should block PRs: only for deterministic benchmark scripts.
# Paired-mode hint: use paired mode if JIT warmup or runner variance dominates.
#
# Fast first-hour check: a dedicated benchmark script with stable input.
# [[bench]]
# name = "node-bench"
# command = ["node", "scripts/bench.js"]
#
# Package-manager path: useful when `npm run bench` already exists.
# [[bench]]
# name = "npm-bench"
# command = ["npm", "run", "bench"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::PythonCommand => r#"
# Benchmark recipe: python-command
# Review and edit before committing. These are candidates, not policy.
# Best for: dedicated Python benchmark scripts with fixed input and environment.
# Bad for: dependency installation, network calls, and correctness-only test runs.
# Expected noise: medium when interpreter startup or environment setup dominates.
# Recommended mode: advisory until calibrated; gate only stable workloads.
# Should block PRs: only after repeat count and baseline maturity are proven.
# Paired-mode hint: use paired mode if interpreter startup or host variance dominates.
#
# Fast first-hour check: a dedicated benchmark script with stable input.
# [[bench]]
# name = "python-bench"
# command = ["python", "scripts/bench.py"]
#
# Module path: useful when the project already exposes a benchmark module.
# [[bench]]
# name = "python-module-bench"
# command = ["python", "-m", "benchmarks"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::HttpSmoke => r#"
# Benchmark recipe: http-smoke
# Review and edit before committing. These are candidates, not policy.
# Best for: local HTTP handlers, smoke latency, and simple service endpoints.
# Bad for: internet calls, shared staging services, and unisolated dependencies.
# Expected noise: medium to high unless the service and host are isolated.
# Recommended mode: advisory first; gate only local isolated endpoints.
# Should block PRs: only after service startup and network noise are controlled.
# Paired-mode hint: use paired mode when service startup or runner networking dominates.
#
# Local endpoint smoke: replace URL with a local service endpoint under test.
# [[bench]]
# name = "http-health"
# command = ["curl", "-fsS", "http://127.0.0.1:8080/health"]
#
# Scripted HTTP benchmark: keep setup and request load deterministic.
# [[bench]]
# name = "http-smoke"
# command = ["./scripts/bench-http.sh"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::GenericCommand => r#"
# Benchmark recipe: generic-command
# Review and edit before committing. These are candidates, not policy.
# Best for: language-neutral command benchmarks with stable local input.
# Bad for: commands that depend on external services or mix setup with runtime.
# Expected noise: unknown until calibrated.
# Recommended mode: advisory until signal maturity is proven.
# Should block PRs: only after baseline and signal maturity are proven.
# Paired-mode hint: use paired mode if repeated local runs disagree.
#
# Fast first-hour check: a stable command that measures the workload directly.
# [[bench]]
# name = "command-smoke"
# command = ["./scripts/bench.sh"]
#
# Language-neutral example: replace this with your real benchmark command.
# [[bench]]
# name = "my-command"
# command = ["your-benchmark-command", "--flag"]
"#
        .to_string(),
    }
}

/// Execute the `init` subcommand.
pub(crate) fn execute_init(args: InitArgs) -> anyhow::Result<()> {
    let preset = match args.preset {
        InitPreset::Standard => Preset::Standard,
        InitPreset::Release => Preset::Release,
        InitPreset::Tier1Fast => Preset::Tier1Fast,
    };

    let ci_platform = args.ci.map(|p| match p {
        InitCiPlatform::Github => CiPlatform::GitHub,
        InitCiPlatform::Gitlab => CiPlatform::GitLab,
        InitCiPlatform::Bitbucket => CiPlatform::Bitbucket,
        InitCiPlatform::Circleci => CiPlatform::CircleCi,
    });

    let scan_dir = if args.dir == Path::new(".") {
        std::env::current_dir().context("cannot determine current directory")?
    } else {
        args.dir.clone()
    };

    if args.output.exists() && !args.yes {
        anyhow::bail!(
            "{} already exists; use --yes to overwrite",
            args.output.display()
        );
    }

    eprintln!("Scanning {} for benchmarks...", scan_dir.display());
    let benchmarks = discover_benchmarks(&scan_dir);

    if benchmarks.is_empty() {
        eprintln!("No benchmarks discovered. The generated config will have no [[bench]] entries.");
        eprintln!("You can add them manually to {}.", args.output.display());
    } else {
        eprintln!("Discovered {} benchmark(s):", benchmarks.len());
        for b in &benchmarks {
            eprintln!("  - {} ({})", b.name, b.source);
        }
    }

    let config = generate_config(&benchmarks, preset);
    let mut toml_content = render_config_toml(&config);
    let suggestion_profile = args
        .suggest_benches
        .map(|profile| resolve_benchmark_suggestion_profile(profile, &scan_dir));
    if let Some(profile) = suggestion_profile {
        toml_content.push_str(&render_benchmark_suggestions(profile));
    }

    fs::write(&args.output, &toml_content)
        .with_context(|| format!("write {}", args.output.display()))?;
    eprintln!("Wrote {}", args.output.display());
    if let Some(profile) = suggestion_profile {
        eprintln!(
            "Appended reviewable benchmark suggestions ({}) to {}.",
            profile.as_str(),
            args.output.display()
        );
        eprintln!("Review and edit suggestions before committing baselines.");
    }

    let baseline_dir = config
        .defaults
        .baseline_dir
        .as_deref()
        .unwrap_or(DEFAULT_FALLBACK_BASELINE_DIR);
    if !is_remote_storage_uri(baseline_dir) {
        let baseline_dir = PathBuf::from(baseline_dir);
        fs::create_dir_all(&baseline_dir)
            .with_context(|| format!("create {}", baseline_dir.display()))?;
        let gitkeep = baseline_dir.join(".gitkeep");
        if !gitkeep.exists() || args.yes {
            fs::write(&gitkeep, "").with_context(|| format!("write {}", gitkeep.display()))?;
            eprintln!("Wrote {}", gitkeep.display());
        }
    }

    let generated_workflow_path = if let Some(platform) = ci_platform {
        let workflow_path = ci_workflow_path(platform);
        let workflow_content = scaffold_ci(platform, &args.output);

        if let Some(parent) = workflow_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }

        fs::write(&workflow_path, &workflow_content)
            .with_context(|| format!("write {}", workflow_path.display()))?;
        eprintln!("Wrote {}", workflow_path.display());
        Some(workflow_path)
    } else {
        None
    };

    let setup_dir = PathBuf::from(".perfgate");
    fs::create_dir_all(&setup_dir).with_context(|| format!("create {}", setup_dir.display()))?;
    let setup_readme = setup_dir.join("README.md");
    if !setup_readme.exists() || args.yes {
        fs::write(
            &setup_readme,
            render_onboarding_readme(
                &args.output,
                generated_workflow_path.as_deref(),
                !benchmarks.is_empty(),
            ),
        )
        .with_context(|| format!("write {}", setup_readme.display()))?;
        eprintln!("Wrote {}", setup_readme.display());
    }

    eprintln!("\nNext:");
    if benchmarks.is_empty() {
        eprintln!(
            "  1. Add at least one [[bench]] entry to {}.",
            args.output.display()
        );
        eprintln!("     Example:");
        eprintln!("       [[bench]]");
        eprintln!("       name = \"my-command\"");
        eprintln!("       command = [\"your-benchmark-command\", \"--flag\"]");
        eprintln!("     Replace the command with what measures this repo, for example:");
        eprintln!("       command = [\"cargo\", \"bench\", \"--bench\", \"my-bench\"]");
        eprintln!("       command = [\"node\", \"scripts/bench.js\"]");
        eprintln!(
            "  2. Run: perfgate check --config {} --all",
            args.output.display()
        );
        eprintln!("  3. Promote a trusted first baseline:");
        eprintln!(
            "     perfgate baseline promote --config {} --all",
            args.output.display()
        );
        if let Some(workflow_path) = &generated_workflow_path {
            eprintln!(
                "  4. Commit {}, {}, baselines/.gitkeep, and .perfgate/README.md",
                args.output.display(),
                workflow_path.display()
            );
        } else {
            eprintln!(
                "  4. Commit {}, baselines/.gitkeep, and .perfgate/README.md",
                args.output.display()
            );
        }
        return Ok(());
    }

    eprintln!(
        "  1. Run: perfgate check --config {} --all",
        args.output.display()
    );
    eprintln!("  2. Promote a trusted first baseline:");
    eprintln!(
        "     perfgate baseline promote --config {} --all",
        args.output.display()
    );
    if let Some(workflow_path) = &generated_workflow_path {
        eprintln!(
            "  3. Commit {}, {}, baselines/.gitkeep, and .perfgate/README.md",
            args.output.display(),
            workflow_path.display()
        );
    } else {
        eprintln!(
            "  3. Commit {}, baselines/.gitkeep, and .perfgate/README.md",
            args.output.display()
        );
    }

    Ok(())
}
