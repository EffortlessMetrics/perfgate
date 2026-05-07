//! Auto-discovery of benchmarks in a repository.
//!
//! Scans a project directory for common benchmark frameworks and returns
//! a list of discovered benchmarks with metadata about framework, language,
//! and suggested command to run them.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// A benchmark discovered by scanning the repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredBenchmark {
    /// Human-readable name for the benchmark.
    pub name: String,
    /// Framework that was detected (e.g. "criterion", "go-bench", "pytest-benchmark").
    pub framework: String,
    /// Suggested command to run this benchmark.
    pub command: String,
    /// Path to the file or directory where this benchmark was found (relative to scan root).
    pub path: String,
    /// Programming language.
    pub language: String,
    /// Confidence level: "high", "medium", or "low".
    pub confidence: String,
}

/// Orchestrates all framework-specific scanners and returns the combined results.
pub fn discover_all(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    results.extend(scan_rust_criterion(root));
    results.extend(scan_go_benchmarks(root));
    results.extend(scan_python_pytest_benchmark(root));
    results.extend(scan_javascript_benchmark(root));
    results.extend(scan_custom_directories(root));
    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

// ---------------------------------------------------------------------------
// Rust / Criterion
// ---------------------------------------------------------------------------

/// Scan for Rust/Criterion benchmarks by looking for `[[bench]]` targets in
/// `Cargo.toml` and `criterion_group!` macros in `benches/`.
fn scan_rust_criterion(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();

    // Strategy 1: Parse Cargo.toml for [[bench]] targets
    let cargo_toml = root.join("Cargo.toml");
    if cargo_toml.is_file()
        && let Ok(content) = fs::read_to_string(&cargo_toml)
    {
        results.extend(parse_cargo_bench_targets(&content, root));
    }

    // Strategy 2: Scan benches/ directory for criterion_group! macros
    let benches_dir = root.join("benches");
    if benches_dir.is_dir() {
        results.extend(scan_dir_for_criterion(&benches_dir, root));
    }

    // Deduplicate by name (Cargo.toml targets take precedence)
    let mut seen = std::collections::HashSet::new();
    results.retain(|b| seen.insert(b.name.clone()));
    results
}

/// Parse `[[bench]]` entries from a Cargo.toml string.
fn parse_cargo_bench_targets(content: &str, root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();

    // Simple line-based parser for [[bench]] sections.
    // We look for `[[bench]]` headers and then `name = "..."` lines.
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed == "[[bench]]" {
            let mut name = None;
            let mut harness = true;
            let mut j = i + 1;
            while j < lines.len() {
                let ltrimmed = lines[j].trim();
                if ltrimmed.starts_with('[') {
                    break;
                }
                if let Some(val) = extract_toml_string_value(ltrimmed, "name") {
                    name = Some(val);
                }
                if ltrimmed.starts_with("harness") && ltrimmed.contains("false") {
                    harness = false;
                }
                j += 1;
            }
            if let Some(bench_name) = name {
                let confidence = if harness { "medium" } else { "high" };
                let framework = if harness { "rust-bench" } else { "criterion" };
                let command = format!("cargo bench --bench {bench_name}");

                // Determine path
                let bench_path = root.join("benches").join(format!("{bench_name}.rs"));
                let rel_path = if bench_path.exists() {
                    format!("benches/{bench_name}.rs")
                } else {
                    "Cargo.toml".to_string()
                };

                results.push(DiscoveredBenchmark {
                    name: bench_name,
                    framework: framework.to_string(),
                    command,
                    path: rel_path,
                    language: "rust".to_string(),
                    confidence: confidence.to_string(),
                });
            }
            i = j;
        } else {
            i += 1;
        }
    }
    results
}

/// Extract a string value from a TOML `key = "value"` line.
fn extract_toml_string_value(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with(key) {
        return None;
    }
    let rest = trimmed[key.len()..].trim();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim();
    let rest = rest.strip_prefix('"')?;
    let rest = rest.strip_suffix('"')?;
    Some(rest.to_string())
}

/// Recursively scan a directory for `.rs` files containing `criterion_group!`.
fn scan_dir_for_criterion(dir: &Path, root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            results.extend(scan_dir_for_criterion(&path, root));
        } else if path.extension().is_some_and(|ext| ext == "rs")
            && let Ok(content) = fs::read_to_string(&path)
            && (content.contains("criterion_group!") || content.contains("criterion_main!"))
        {
            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let bench_name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            results.push(DiscoveredBenchmark {
                name: bench_name.clone(),
                framework: "criterion".to_string(),
                command: format!("cargo bench --bench {bench_name}"),
                path: rel_path,
                language: "rust".to_string(),
                confidence: "high".to_string(),
            });
        }
    }
    results
}

// ---------------------------------------------------------------------------
// Go benchmarks
// ---------------------------------------------------------------------------

/// Scan for Go benchmarks by looking for `func Benchmark` in `*_test.go` files.
fn scan_go_benchmarks(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    walk_files(root, &mut |path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.ends_with("_test.go")
            && let Ok(content) = fs::read_to_string(path)
        {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("func Benchmark")
                    && let Some(paren_pos) = rest.find('(')
                {
                    let func_name = rest[..paren_pos].trim();
                    if !func_name.is_empty()
                        && func_name.chars().next().is_some_and(|c| c.is_uppercase())
                    {
                        let full_name = format!("Benchmark{func_name}");
                        let rel_path = path
                            .strip_prefix(root)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        let pkg_dir = path
                            .parent()
                            .and_then(|p| p.strip_prefix(root).ok())
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_else(|| ".".to_string());
                        results.push(DiscoveredBenchmark {
                            name: full_name.clone(),
                            framework: "go-bench".to_string(),
                            command: format!("go test -bench=^{full_name}$ -benchmem ./{pkg_dir}"),
                            path: rel_path,
                            language: "go".to_string(),
                            confidence: "high".to_string(),
                        });
                    }
                }
            }
        }
    });
    results
}

// ---------------------------------------------------------------------------
// Python / pytest-benchmark
// ---------------------------------------------------------------------------

/// Scan for Python pytest-benchmark usage by looking for `benchmark` fixture in test files.
fn scan_python_pytest_benchmark(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    walk_files(root, &mut |path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if (name.starts_with("test_") || name.ends_with("_test.py") || name.starts_with("bench_"))
            && name.ends_with(".py")
            && let Ok(content) = fs::read_to_string(path)
            && content.contains("benchmark")
            && content.contains("def ")
        {
            // Look for functions that use the benchmark fixture
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("def ")
                    && let Some(paren_pos) = rest.find('(')
                    && rest[paren_pos..].contains("benchmark")
                {
                    let func_name = &rest[..paren_pos];
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    results.push(DiscoveredBenchmark {
                        name: func_name.to_string(),
                        framework: "pytest-benchmark".to_string(),
                        command: format!("pytest --benchmark-only {rel_path}::{func_name}"),
                        path: rel_path,
                        language: "python".to_string(),
                        confidence: "high".to_string(),
                    });
                }
            }
        }
    });
    results
}

// ---------------------------------------------------------------------------
// JavaScript / Benchmark.js
// ---------------------------------------------------------------------------

/// Scan for JavaScript Benchmark.js usage by looking for `suite.add` patterns.
fn scan_javascript_benchmark(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    walk_files(root, &mut |path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if (name.ends_with(".js") || name.ends_with(".mjs"))
            && (name.contains("bench") || name.contains("perf"))
            && let Ok(content) = fs::read_to_string(path)
            && (content.contains("suite.add") || content.contains("Suite"))
        {
            let rel_path = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let bench_name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            results.push(DiscoveredBenchmark {
                name: bench_name,
                framework: "benchmark.js".to_string(),
                command: format!("node {rel_path}"),
                path: rel_path,
                language: "javascript".to_string(),
                confidence: "medium".to_string(),
            });
        }
    });
    results
}

// ---------------------------------------------------------------------------
// Custom executable directories
// ---------------------------------------------------------------------------

/// Scan well-known directories (`benchmarks/`, `bench/`, `perf/`) for executable files.
fn scan_custom_directories(root: &Path) -> Vec<DiscoveredBenchmark> {
    let mut results = Vec::new();
    let dirs = ["benchmarks", "bench", "perf"];
    for dir_name in &dirs {
        let dir = root.join(dir_name);
        if dir.is_dir() {
            let entries = match fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_likely_executable(&path) {
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    let bench_name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    results.push(DiscoveredBenchmark {
                        name: bench_name,
                        framework: "custom".to_string(),
                        command: format!("./{rel_path}"),
                        path: rel_path,
                        language: "unknown".to_string(),
                        confidence: "low".to_string(),
                    });
                }
            }
        }
    }
    results
}

/// Heuristic to decide if a file is likely executable.
/// On Unix we would check permissions; on all platforms we check for
/// script shebangs or known executable extensions.
fn is_likely_executable(path: &Path) -> bool {
    // Check known executable extensions
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        if matches!(
            ext.as_str(),
            "sh" | "bash" | "zsh" | "py" | "rb" | "pl" | "exe" | "bat" | "cmd" | "ps1"
        ) {
            return true;
        }
    }

    // Check for shebang
    if let Ok(content) = fs::read(path)
        && content.starts_with(b"#!")
    {
        return true;
    }

    // No extension might be a compiled binary
    path.extension().is_none()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Walk files under `root`, skipping common non-source directories.
fn walk_files(root: &Path, callback: &mut dyn FnMut(&Path)) {
    walk_files_inner(root, root, callback);
}

fn walk_files_inner(dir: &Path, root: &Path, callback: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            // Skip common non-source directories
            if matches!(
                name.as_ref(),
                "target"
                    | "node_modules"
                    | ".git"
                    | ".hg"
                    | ".svn"
                    | "__pycache__"
                    | "vendor"
                    | "dist"
                    | "build"
                    | ".tox"
                    | ".venv"
                    | "venv"
            ) {
                continue;
            }
            // Limit depth: only go ~4 levels deep relative to root
            let depth = path
                .strip_prefix(root)
                .map(|p| p.components().count())
                .unwrap_or(0);
            if depth < 5 {
                walk_files_inner(&path, root, callback);
            }
        } else {
            callback(&path);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper to create a temp directory with files.
    fn setup_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("failed to create temp dir")
    }

    #[test]
    fn test_discover_empty_dir() {
        let tmp = setup_temp_dir();
        let results = discover_all(tmp.path());
        assert!(results.is_empty());
    }

    #[test]
    fn test_extract_toml_string_value() {
        assert_eq!(
            extract_toml_string_value(r#"name = "my_bench""#, "name"),
            Some("my_bench".to_string())
        );
        assert_eq!(
            extract_toml_string_value(r#"  name = "spaced"  "#, "name"),
            Some("spaced".to_string())
        );
        assert_eq!(
            extract_toml_string_value(r#"harness = false"#, "name"),
            None
        );
        assert_eq!(
            extract_toml_string_value(r#"name = "has_underscore""#, "name"),
            Some("has_underscore".to_string())
        );
    }

    #[test]
    fn test_parse_cargo_bench_targets() {
        let content = r#"
[package]
name = "myproject"

[[bench]]
name = "my_bench"
harness = false

[[bench]]
name = "another"
"#;
        let tmp = setup_temp_dir();
        let results = parse_cargo_bench_targets(content, tmp.path());
        assert_eq!(results.len(), 2);

        assert_eq!(results[0].name, "my_bench");
        assert_eq!(results[0].framework, "criterion");
        assert_eq!(results[0].confidence, "high");

        assert_eq!(results[1].name, "another");
        assert_eq!(results[1].framework, "rust-bench");
        assert_eq!(results[1].confidence, "medium");
    }

    #[test]
    fn test_scan_criterion_file() {
        let tmp = setup_temp_dir();
        let benches_dir = tmp.path().join("benches");
        fs::create_dir(&benches_dir).unwrap();
        fs::write(
            benches_dir.join("sort_bench.rs"),
            r#"
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_sort(c: &mut Criterion) {
    c.bench_function("sort_1000", |b| {
        b.iter(|| {
            let mut v: Vec<i32> = (0..1000).rev().collect();
            v.sort();
        })
    });
}

criterion_group!(benches, bench_sort);
criterion_main!(benches);
"#,
        )
        .unwrap();

        let results = scan_rust_criterion(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "sort_bench");
        assert_eq!(results[0].framework, "criterion");
        assert_eq!(results[0].language, "rust");
        assert_eq!(results[0].confidence, "high");
        assert!(
            results[0]
                .command
                .contains("cargo bench --bench sort_bench")
        );
    }

    #[test]
    fn test_scan_go_benchmarks() {
        let tmp = setup_temp_dir();
        fs::write(
            tmp.path().join("sort_test.go"),
            r#"
package main

import "testing"

func BenchmarkSort(b *testing.B) {
    for i := 0; i < b.N; i++ {
        // sort something
    }
}

func BenchmarkSearch(b *testing.B) {
    for i := 0; i < b.N; i++ {
        // search something
    }
}

func TestNotABenchmark(t *testing.T) {}
"#,
        )
        .unwrap();

        let results = scan_go_benchmarks(tmp.path());
        assert_eq!(results.len(), 2);

        let names: Vec<&str> = results.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"BenchmarkSort"));
        assert!(names.contains(&"BenchmarkSearch"));

        for b in &results {
            assert_eq!(b.framework, "go-bench");
            assert_eq!(b.language, "go");
            assert_eq!(b.confidence, "high");
            assert!(b.command.contains("go test -bench="));
        }
    }

    #[test]
    fn test_scan_python_pytest_benchmark() {
        let tmp = setup_temp_dir();
        fs::write(
            tmp.path().join("test_perf.py"),
            r#"
def test_sort_speed(benchmark):
    benchmark(sorted, list(range(1000, 0, -1)))

def test_not_a_benchmark():
    assert True
"#,
        )
        .unwrap();

        let results = scan_python_pytest_benchmark(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test_sort_speed");
        assert_eq!(results[0].framework, "pytest-benchmark");
        assert_eq!(results[0].language, "python");
        assert_eq!(results[0].confidence, "high");
    }

    #[test]
    fn test_scan_javascript_benchmark() {
        let tmp = setup_temp_dir();
        fs::write(
            tmp.path().join("bench.js"),
            r#"
const Benchmark = require('benchmark');
const suite = new Benchmark.Suite;

suite.add('sort', function() {
    [3,1,2].sort();
})
.run();
"#,
        )
        .unwrap();

        let results = scan_javascript_benchmark(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "bench");
        assert_eq!(results[0].framework, "benchmark.js");
        assert_eq!(results[0].language, "javascript");
        assert_eq!(results[0].confidence, "medium");
    }

    #[test]
    fn test_scan_custom_directories() {
        let tmp = setup_temp_dir();
        let bench_dir = tmp.path().join("benchmarks");
        fs::create_dir(&bench_dir).unwrap();
        fs::write(bench_dir.join("run_perf.sh"), "#!/bin/bash\necho hello").unwrap();
        fs::write(bench_dir.join("README.md"), "# Benchmarks").unwrap();

        let results = scan_custom_directories(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "run_perf");
        assert_eq!(results[0].framework, "custom");
        assert_eq!(results[0].confidence, "low");
    }

    #[test]
    fn test_is_likely_executable() {
        let tmp = setup_temp_dir();
        let sh_file = tmp.path().join("run.sh");
        fs::write(&sh_file, "#!/bin/bash\necho hello").unwrap();
        assert!(is_likely_executable(&sh_file));

        let py_file = tmp.path().join("bench.py");
        fs::write(&py_file, "print('hello')").unwrap();
        assert!(is_likely_executable(&py_file));

        let md_file = tmp.path().join("README.md");
        fs::write(&md_file, "# Hello").unwrap();
        assert!(!is_likely_executable(&md_file));

        // File with no extension and shebang
        let shebang_file = tmp.path().join("my_bench");
        fs::write(&shebang_file, "#!/usr/bin/env python\nprint('bench')").unwrap();
        assert!(is_likely_executable(&shebang_file));
    }

    #[test]
    fn test_discover_all_mixed() {
        let tmp = setup_temp_dir();

        // Add a Cargo.toml with bench target
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"
[package]
name = "test"

[[bench]]
name = "perf_test"
harness = false
"#,
        )
        .unwrap();

        // Add a Go benchmark
        fs::write(
            tmp.path().join("algo_test.go"),
            r#"
package algo

import "testing"

func BenchmarkAlgo(b *testing.B) {}
"#,
        )
        .unwrap();

        let results = discover_all(tmp.path());
        assert!(results.len() >= 2);

        let names: Vec<&str> = results.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"perf_test"));
        assert!(names.contains(&"BenchmarkAlgo"));
    }

    #[test]
    fn test_discover_all_sorted_by_name() {
        let tmp = setup_temp_dir();
        let bench_dir = tmp.path().join("benchmarks");
        fs::create_dir(&bench_dir).unwrap();
        fs::write(bench_dir.join("zebra.sh"), "#!/bin/bash").unwrap();
        fs::write(bench_dir.join("alpha.sh"), "#!/bin/bash").unwrap();

        let results = discover_all(tmp.path());
        if results.len() >= 2 {
            assert!(results[0].name <= results[1].name);
        }
    }

    #[test]
    fn test_cargo_bench_targets_dedup() {
        // If Cargo.toml declares a bench AND the .rs file has criterion_group!,
        // we should only get one entry.
        let tmp = setup_temp_dir();
        fs::write(
            tmp.path().join("Cargo.toml"),
            r#"
[package]
name = "test"

[[bench]]
name = "my_bench"
harness = false
"#,
        )
        .unwrap();

        let benches_dir = tmp.path().join("benches");
        fs::create_dir(&benches_dir).unwrap();
        fs::write(
            benches_dir.join("my_bench.rs"),
            "criterion_group!(benches, f);\ncriterion_main!(benches);",
        )
        .unwrap();

        let results = scan_rust_criterion(tmp.path());
        // Should deduplicate: only one entry named "my_bench"
        let count = results.iter().filter(|b| b.name == "my_bench").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_walk_files_skips_git_dir() {
        let tmp = setup_temp_dir();
        let git_dir = tmp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("config"), "core").unwrap();

        let mut visited = Vec::new();
        walk_files(tmp.path(), &mut |path| {
            visited.push(path.to_path_buf());
        });
        assert!(
            visited.is_empty(),
            "should not visit files inside .git directory"
        );
    }
}
