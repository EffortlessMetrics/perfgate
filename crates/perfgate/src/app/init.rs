//! Benchmark discovery and config generation for `perfgate init`.
//!
//! Scans a repository to detect benchmark targets and generates
//! a `perfgate.toml` configuration file.

use perfgate_types::{BenchConfigFile, ConfigFile, DefaultsConfig};
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// How a benchmark was discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchSource {
    /// A `[[bench]]` target in `Cargo.toml`.
    CargoTarget,
    /// Detected via `criterion_group!` / `criterion_main!` macros.
    Criterion,
    /// Go `func Benchmark*` in `*_test.go`.
    GoBench,
    /// Python pytest-benchmark detected.
    PytestBenchmark,
    /// Fallback / user-supplied.
    Custom,
}

impl fmt::Display for BenchSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchSource::CargoTarget => write!(f, "cargo bench target"),
            BenchSource::Criterion => write!(f, "criterion benchmark"),
            BenchSource::GoBench => write!(f, "go benchmark"),
            BenchSource::PytestBenchmark => write!(f, "pytest-benchmark"),
            BenchSource::Custom => write!(f, "custom"),
        }
    }
}

/// A benchmark discovered by scanning the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredBench {
    pub name: String,
    pub command: Vec<String>,
    pub source: BenchSource,
}

/// Budget preset for config generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Standard,
    Release,
    Tier1Fast,
}

impl Preset {
    pub fn defaults(self) -> DefaultsConfig {
        match self {
            Preset::Standard => DefaultsConfig {
                repeat: Some(5),
                warmup: Some(1),
                threshold: Some(0.20),
                ..DefaultsConfig::default()
            },
            Preset::Release => DefaultsConfig {
                repeat: Some(10),
                warmup: Some(2),
                threshold: Some(0.10),
                ..DefaultsConfig::default()
            },
            Preset::Tier1Fast => DefaultsConfig {
                repeat: Some(3),
                warmup: Some(1),
                threshold: Some(0.30),
                ..DefaultsConfig::default()
            },
        }
    }
}

/// CI platform for workflow scaffolding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiPlatform {
    GitHub,
    GitLab,
    Bitbucket,
    CircleCi,
}

// ---------------------------------------------------------------------------
// Benchmark discovery
// ---------------------------------------------------------------------------

/// Scan `root` for benchmarks.  Does not recurse into hidden or `target` dirs.
pub fn discover_benchmarks(root: &Path) -> Vec<DiscoveredBench> {
    let mut found: Vec<DiscoveredBench> = Vec::new();

    discover_rust_benches(root, &mut found);
    discover_go_benches(root, &mut found);
    discover_python_benches(root, &mut found);

    // De-duplicate by name (first wins).
    let mut seen = std::collections::HashSet::new();
    found.retain(|b| seen.insert(b.name.clone()));

    found
}

// -- Rust / Cargo ----------------------------------------------------------

fn discover_rust_benches(root: &Path, out: &mut Vec<DiscoveredBench>) {
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return;
    }

    let content = match std::fs::read_to_string(&cargo_toml) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Parse [[bench]] targets.
    if let Ok(parsed) = content.parse::<toml::Table>()
        && let Some(toml::Value::Array(benches)) = parsed.get("bench")
    {
        for bench in benches {
            if let Some(name) = bench.get("name").and_then(|v| v.as_str()) {
                let harness = bench
                    .get("harness")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let source = if harness {
                    BenchSource::CargoTarget
                } else {
                    // harness = false usually means Criterion or custom runner
                    BenchSource::Criterion
                };

                out.push(DiscoveredBench {
                    name: name.to_string(),
                    command: vec![
                        "cargo".into(),
                        "bench".into(),
                        "--bench".into(),
                        name.to_string(),
                    ],
                    source,
                });
            }
        }
    }

    // Scan benches/ directory for criterion macros.
    let benches_dir = root.join("benches");
    if benches_dir.is_dir() {
        scan_dir_for_criterion(&benches_dir, out);
    }
}

fn scan_dir_for_criterion(dir: &Path, out: &mut Vec<DiscoveredBench>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("criterion_group!") || content.contains("criterion_main!") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("benchmark");

            // Only add if not already discovered via [[bench]].
            if !out.iter().any(|b| b.name == stem) {
                out.push(DiscoveredBench {
                    name: stem.to_string(),
                    command: vec![
                        "cargo".into(),
                        "bench".into(),
                        "--bench".into(),
                        stem.to_string(),
                    ],
                    source: BenchSource::Criterion,
                });
            }
        }
    }
}

// -- Go --------------------------------------------------------------------

fn discover_go_benches(root: &Path, out: &mut Vec<DiscoveredBench>) {
    // Look for go.mod first.
    if !root.join("go.mod").is_file() {
        return;
    }

    walk_for_go_bench_files(root, root, out);
}

fn walk_for_go_bench_files(root: &Path, dir: &Path, out: &mut Vec<DiscoveredBench>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with('.') || name == "vendor" || name == "node_modules" {
                continue;
            }
            walk_for_go_bench_files(root, &path, out);
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !file_name.ends_with("_test.go") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("func Benchmark") {
            let pkg_dir = path.parent().unwrap_or(root);
            let rel = pkg_dir
                .strip_prefix(root)
                .unwrap_or(pkg_dir)
                .to_string_lossy()
                .replace('\\', "/");

            let pkg = if rel.is_empty() {
                ".".to_string()
            } else {
                format!("./{rel}")
            };

            let bench_name = format!("go-bench-{}", rel.replace('/', "-")).replace("..", "root");
            let bench_name = if bench_name == "go-bench-" {
                "go-bench".to_string()
            } else {
                bench_name
            };

            if !out.iter().any(|b| b.name == bench_name) {
                out.push(DiscoveredBench {
                    name: bench_name,
                    command: vec![
                        "go".into(),
                        "test".into(),
                        "-bench=.".into(),
                        "-benchmem".into(),
                        pkg,
                    ],
                    source: BenchSource::GoBench,
                });
            }
        }
    }
}

// -- Python ----------------------------------------------------------------

fn discover_python_benches(root: &Path, out: &mut Vec<DiscoveredBench>) {
    let markers = [
        "requirements.txt",
        "requirements-dev.txt",
        "requirements-test.txt",
        "setup.py",
        "setup.cfg",
        "pyproject.toml",
    ];

    let mut has_pytest_benchmark = false;
    for marker in &markers {
        let path = root.join(marker);
        if let Ok(content) = std::fs::read_to_string(&path)
            && (content.contains("pytest-benchmark") || content.contains("pytest_benchmark"))
        {
            has_pytest_benchmark = true;
            break;
        }
    }

    // Also check for conftest.py with benchmark fixture usage.
    if !has_pytest_benchmark {
        let conftest = root.join("conftest.py");
        if let Ok(content) = std::fs::read_to_string(&conftest)
            && content.contains("benchmark")
        {
            has_pytest_benchmark = true;
        }
    }

    if has_pytest_benchmark {
        out.push(DiscoveredBench {
            name: "pytest-bench".to_string(),
            command: vec![
                "pytest".into(),
                "--benchmark-only".into(),
                "--benchmark-json=benchmark.json".into(),
            ],
            source: BenchSource::PytestBenchmark,
        });
    }
}

// ---------------------------------------------------------------------------
// Config generation
// ---------------------------------------------------------------------------

/// Build a `ConfigFile` from discovered benchmarks and a preset.
pub fn generate_config(benchmarks: &[DiscoveredBench], preset: Preset) -> ConfigFile {
    let defaults = preset.defaults();

    let benches: Vec<BenchConfigFile> = benchmarks
        .iter()
        .map(|b| BenchConfigFile {
            name: b.name.clone(),
            command: b.command.clone(),
            cwd: None,
            work: None,
            timeout: None,
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
            scaling: None,
        })
        .collect();

    ConfigFile {
        defaults,
        benches,
        ..ConfigFile::default()
    }
}

/// Render a `ConfigFile` to a well-commented TOML string.
pub fn render_config_toml(config: &ConfigFile) -> String {
    let mut out = String::new();

    out.push_str("# perfgate.toml — generated by `perfgate init`\n");
    out.push_str("#\n");
    out.push_str("# Documentation: https://github.com/EffortlessMetrics/perfgate\n\n");

    // [defaults]
    out.push_str("# Default settings applied to all benchmarks unless overridden.\n");
    out.push_str("[defaults]\n");
    if let Some(repeat) = config.defaults.repeat {
        out.push_str(&format!(
            "# Number of measured samples per benchmark run.\n\
             repeat = {repeat}\n"
        ));
    }
    if let Some(warmup) = config.defaults.warmup {
        out.push_str(&format!(
            "# Warmup iterations excluded from statistics.\n\
             warmup = {warmup}\n"
        ));
    }
    if let Some(threshold) = config.defaults.threshold {
        out.push_str(&format!(
            "# Maximum allowed regression fraction (0.20 = 20%).\n\
             threshold = {threshold:.2}\n"
        ));
    }
    if let Some(ref out_dir) = config.defaults.out_dir {
        out.push_str(&format!("out_dir = \"{out_dir}\"\n"));
    }
    if let Some(ref baseline_dir) = config.defaults.baseline_dir {
        out.push_str(&format!("baseline_dir = \"{baseline_dir}\"\n"));
    }

    // [[bench]] entries
    for bench in &config.benches {
        out.push_str(&format!("\n[[bench]]\nname = \"{}\"\n", bench.name));

        // Format command as TOML array.
        let parts: Vec<String> = bench.command.iter().map(|c| format!("\"{c}\"")).collect();
        out.push_str(&format!("command = [{}]\n", parts.join(", ")));

        if let Some(ref cwd) = bench.cwd {
            out.push_str(&format!("cwd = \"{cwd}\"\n"));
        }
        if let Some(repeat) = bench.repeat {
            out.push_str(&format!("repeat = {repeat}\n"));
        }
        if let Some(warmup) = bench.warmup {
            out.push_str(&format!("warmup = {warmup}\n"));
        }
        if let Some(ref timeout) = bench.timeout {
            out.push_str(&format!("timeout = \"{timeout}\"\n"));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// CI scaffold
// ---------------------------------------------------------------------------

/// Generate CI workflow content for the given platform.
pub fn scaffold_ci(platform: CiPlatform, config_path: &Path) -> String {
    let config_str = config_path.to_string_lossy().replace('\\', "/");
    match platform {
        CiPlatform::GitHub => scaffold_github(&config_str),
        CiPlatform::GitLab => scaffold_gitlab(&config_str),
        CiPlatform::Bitbucket => scaffold_bitbucket(&config_str),
        CiPlatform::CircleCi => scaffold_circleci(&config_str),
    }
}

fn scaffold_github(config_path: &str) -> String {
    format!(
        r#"# .github/workflows/perfgate.yml — generated by `perfgate init`
name: Performance Gate

on:
  pull_request:
    branches: [main]

permissions:
  pull-requests: write

jobs:
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install perfgate
        run: cargo install perfgate-cli

      - name: Run benchmarks
        run: perfgate check --config {config_path} --all --mode cockpit --out-dir artifacts/perfgate

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: perfgate
          path: artifacts/perfgate/

      - name: Post PR comment
        if: github.event_name == 'pull_request'
        run: |
          if [ -f artifacts/perfgate/comment.md ]; then
            gh pr comment ${{{{ github.event.pull_request.number }}}} --body-file artifacts/perfgate/comment.md
          fi
        env:
          GH_TOKEN: ${{{{ secrets.GITHUB_TOKEN }}}}
"#
    )
}

fn scaffold_gitlab(config_path: &str) -> String {
    format!(
        r#"# .gitlab-ci.yml snippet — generated by `perfgate init`
perfgate:
  stage: test
  script:
    - cargo install perfgate-cli
    - perfgate check --config {config_path} --all --mode cockpit --out-dir artifacts/perfgate
  artifacts:
    paths:
      - artifacts/perfgate/
    when: always
"#
    )
}

fn scaffold_bitbucket(config_path: &str) -> String {
    format!(
        r#"# bitbucket-pipelines.yml snippet — generated by `perfgate init`
pipelines:
  pull-requests:
    '**':
      - step:
          name: Performance Gate
          script:
            - cargo install perfgate-cli
            - perfgate check --config {config_path} --all --mode cockpit --out-dir artifacts/perfgate
          artifacts:
            - artifacts/perfgate/**
"#
    )
}

fn scaffold_circleci(config_path: &str) -> String {
    format!(
        r#"# .circleci/config.yml snippet — generated by `perfgate init`
version: 2.1
jobs:
  perfgate:
    docker:
      - image: cimg/rust:1.80
    steps:
      - checkout
      - run:
          name: Install perfgate
          command: cargo install perfgate-cli
      - run:
          name: Run benchmarks
          command: perfgate check --config {config_path} --all --mode cockpit --out-dir artifacts/perfgate
      - store_artifacts:
          path: artifacts/perfgate
"#
    )
}

/// Return the default CI workflow file path for a platform.
pub fn ci_workflow_path(platform: CiPlatform) -> PathBuf {
    match platform {
        CiPlatform::GitHub => PathBuf::from(".github/workflows/perfgate.yml"),
        CiPlatform::GitLab => PathBuf::from(".gitlab-ci.perfgate.yml"),
        CiPlatform::Bitbucket => PathBuf::from("bitbucket-pipelines.perfgate.yml"),
        CiPlatform::CircleCi => PathBuf::from(".circleci/perfgate.yml"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // -- Preset defaults ---------------------------------------------------

    #[test]
    fn preset_standard_defaults() {
        let d = Preset::Standard.defaults();
        assert_eq!(d.repeat, Some(5));
        assert_eq!(d.warmup, Some(1));
        assert_eq!(d.threshold, Some(0.20));
    }

    #[test]
    fn preset_release_defaults() {
        let d = Preset::Release.defaults();
        assert_eq!(d.repeat, Some(10));
        assert_eq!(d.warmup, Some(2));
        assert_eq!(d.threshold, Some(0.10));
    }

    #[test]
    fn preset_tier1fast_defaults() {
        let d = Preset::Tier1Fast.defaults();
        assert_eq!(d.repeat, Some(3));
        assert_eq!(d.warmup, Some(1));
        assert_eq!(d.threshold, Some(0.30));
    }

    // -- Rust / Cargo discovery -------------------------------------------

    #[test]
    fn discover_cargo_bench_targets() {
        let dir = tempfile::tempdir().unwrap();
        let cargo = dir.path().join("Cargo.toml");
        fs::write(
            &cargo,
            r#"
[package]
name = "example"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "my-bench"
harness = false
"#,
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "my-bench");
        assert_eq!(found[0].source, BenchSource::Criterion); // harness=false
        assert_eq!(
            found[0].command,
            vec!["cargo", "bench", "--bench", "my-bench"]
        );
    }

    #[test]
    fn discover_cargo_bench_harness_true() {
        let dir = tempfile::tempdir().unwrap();
        let cargo = dir.path().join("Cargo.toml");
        fs::write(
            &cargo,
            r#"
[package]
name = "example"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "basic"
"#,
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, BenchSource::CargoTarget);
    }

    #[test]
    fn discover_criterion_from_benches_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Need a Cargo.toml so the Rust scanner fires.
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();

        let benches_dir = dir.path().join("benches");
        fs::create_dir(&benches_dir).unwrap();
        fs::write(
            benches_dir.join("perf.rs"),
            "criterion_group!(benches, bench_fn);\ncriterion_main!(benches);\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "perf");
        assert_eq!(found[0].source, BenchSource::Criterion);
    }

    #[test]
    fn criterion_dedup_with_cargo_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "x"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "perf"
harness = false
"#,
        )
        .unwrap();

        let benches_dir = dir.path().join("benches");
        fs::create_dir(&benches_dir).unwrap();
        fs::write(
            benches_dir.join("perf.rs"),
            "criterion_group!(benches, bench_fn);\ncriterion_main!(benches);\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        // Should only appear once.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "perf");
    }

    // -- Go discovery ------------------------------------------------------

    #[test]
    fn discover_go_benches() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module example\n").unwrap();
        fs::write(
            dir.path().join("bench_test.go"),
            "package main\n\nfunc BenchmarkFoo(b *testing.B) {\n}\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "go-bench");
        assert_eq!(found[0].source, BenchSource::GoBench);
        assert!(found[0].command.contains(&"-bench=.".to_string()));
    }

    #[test]
    fn discover_go_benches_in_subpackage() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("go.mod"), "module example\n").unwrap();
        let sub = dir.path().join("pkg").join("fast");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("bench_test.go"),
            "package fast\nfunc BenchmarkBar(b *testing.B) {}\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "go-bench-pkg-fast");
    }

    // -- Python discovery --------------------------------------------------

    #[test]
    fn discover_pytest_benchmark_from_requirements() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("requirements.txt"),
            "pytest\npytest-benchmark\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "pytest-bench");
        assert_eq!(found[0].source, BenchSource::PytestBenchmark);
    }

    #[test]
    fn discover_pytest_benchmark_from_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "[project.optional-dependencies]\ntest = [\"pytest-benchmark\"]\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, BenchSource::PytestBenchmark);
    }

    #[test]
    fn discover_pytest_benchmark_from_conftest() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("conftest.py"),
            "def test_speed(benchmark):\n    benchmark(lambda: None)\n",
        )
        .unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 1);
    }

    // -- Empty repo --------------------------------------------------------

    #[test]
    fn empty_repo_discovers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let found = discover_benchmarks(dir.path());
        assert!(found.is_empty());
    }

    // -- Config generation -------------------------------------------------

    #[test]
    fn generate_config_produces_valid_toml() {
        let benches = vec![
            DiscoveredBench {
                name: "my-bench".into(),
                command: vec!["cargo".into(), "bench".into()],
                source: BenchSource::CargoTarget,
            },
            DiscoveredBench {
                name: "go-bench".into(),
                command: vec!["go".into(), "test".into(), "-bench=.".into(), ".".into()],
                source: BenchSource::GoBench,
            },
        ];

        let config = generate_config(&benches, Preset::Standard);
        assert_eq!(config.benches.len(), 2);
        assert_eq!(config.defaults.repeat, Some(5));
        assert_eq!(config.defaults.threshold, Some(0.20));
    }

    #[test]
    fn render_config_toml_roundtrip() {
        let benches = vec![DiscoveredBench {
            name: "my-bench".into(),
            command: vec![
                "cargo".into(),
                "bench".into(),
                "--bench".into(),
                "my-bench".into(),
            ],
            source: BenchSource::CargoTarget,
        }];

        let config = generate_config(&benches, Preset::Release);
        let toml_str = render_config_toml(&config);

        // The rendered TOML must parse back without error.
        let parsed: ConfigFile = toml::from_str(&toml_str).expect("rendered TOML should parse");
        assert_eq!(parsed.benches.len(), 1);
        assert_eq!(parsed.benches[0].name, "my-bench");
        assert_eq!(parsed.defaults.repeat, Some(10));
        assert_eq!(parsed.defaults.threshold, Some(0.10));
    }

    // -- CI scaffold -------------------------------------------------------

    #[test]
    fn scaffold_github_ci() {
        let content = scaffold_ci(CiPlatform::GitHub, Path::new("perfgate.toml"));
        assert!(content.contains("perfgate check"));
        assert!(content.contains("perfgate.toml"));
        assert!(content.contains("ubuntu-latest"));
    }

    #[test]
    fn scaffold_gitlab_ci() {
        let content = scaffold_ci(CiPlatform::GitLab, Path::new("perfgate.toml"));
        assert!(content.contains("perfgate check"));
        assert!(content.contains("stage: test"));
    }

    #[test]
    fn scaffold_bitbucket_ci() {
        let content = scaffold_ci(CiPlatform::Bitbucket, Path::new("perfgate.toml"));
        assert!(content.contains("perfgate check"));
        assert!(content.contains("pipelines"));
    }

    #[test]
    fn scaffold_circleci_ci() {
        let content = scaffold_ci(CiPlatform::CircleCi, Path::new("perfgate.toml"));
        assert!(content.contains("perfgate check"));
        assert!(content.contains("version: 2.1"));
    }

    #[test]
    fn ci_workflow_paths() {
        assert_eq!(
            ci_workflow_path(CiPlatform::GitHub),
            PathBuf::from(".github/workflows/perfgate.yml")
        );
        assert_eq!(
            ci_workflow_path(CiPlatform::GitLab),
            PathBuf::from(".gitlab-ci.perfgate.yml")
        );
    }

    // -- BenchSource display -----------------------------------------------

    #[test]
    fn bench_source_display() {
        assert_eq!(
            format!("{}", BenchSource::CargoTarget),
            "cargo bench target"
        );
        assert_eq!(format!("{}", BenchSource::Criterion), "criterion benchmark");
        assert_eq!(format!("{}", BenchSource::GoBench), "go benchmark");
        assert_eq!(
            format!("{}", BenchSource::PytestBenchmark),
            "pytest-benchmark"
        );
        assert_eq!(format!("{}", BenchSource::Custom), "custom");
    }

    // -- Mixed repo discovery ----------------------------------------------

    #[test]
    fn discover_mixed_repo() {
        let dir = tempfile::tempdir().unwrap();

        // Rust
        fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[package]
name = "mixed"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "rust-bench"
harness = false
"#,
        )
        .unwrap();

        // Go
        fs::write(dir.path().join("go.mod"), "module mixed\n").unwrap();
        fs::write(
            dir.path().join("bench_test.go"),
            "package main\nfunc BenchmarkX(b *testing.B) {}\n",
        )
        .unwrap();

        // Python
        fs::write(dir.path().join("requirements.txt"), "pytest-benchmark\n").unwrap();

        let found = discover_benchmarks(dir.path());
        assert_eq!(found.len(), 3);

        let names: Vec<&str> = found.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"rust-bench"));
        assert!(names.contains(&"go-bench"));
        assert!(names.contains(&"pytest-bench"));
    }
}
