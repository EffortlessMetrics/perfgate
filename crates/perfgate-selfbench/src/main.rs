use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "noop" => Ok(()),
        "cpu-fixed" => cpu_fixed(),
        "io-fixed" => io_fixed(),
        "json-read" => json_read(args.get(2).map(|s| s.as_str())),
        "cli-compare-small" | "cli/compare-small" => cli_compare(
            ".ci/fixtures/compare/small-baseline.json",
            ".ci/fixtures/compare/small-current.json",
        ),
        "cli-compare-large" | "cli/compare-large" => cli_compare(
            ".ci/fixtures/compare/large-baseline.json",
            ".ci/fixtures/compare/large-current.json",
        ),
        "cli-check-single" | "cli/check-single" => cli_check("test-bench"),
        "cli-check-no-baseline" | "cli/check-no-baseline" => cli_check("test-no-baseline"),
        "render-md" | "render/md" => render("md", "comment.md"),
        "render-report" | "render/report" => render("report", "report.json"),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Usage: perfgate-selfbench <command>");
    eprintln!("Commands:");
    eprintln!("  noop, cpu-fixed, io-fixed, json-read [path]");
    eprintln!("  cli-compare-small, cli-compare-large");
    eprintln!("  cli-check-single, cli-check-no-baseline");
    eprintln!("  render-md, render-report");
}

fn cpu_fixed() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut sum = 0u64;
    // Perform a fixed amount of CPU work
    for i in 0..10_000_000 {
        sum = sum.wrapping_add(i);
    }
    println!("CPU work complete: {sum}");
    eprintln!("Duration: {:?}", start.elapsed());
    Ok(())
}

fn io_fixed() -> anyhow::Result<()> {
    let start = Instant::now();
    let tmp_dir = std::env::temp_dir();
    let path = tmp_dir.join("perfgate-selfbench-workload.bin");

    // Write 1MB of data
    let data = vec![0u8; 1024 * 1024];
    fs::write(&path, &data)?;

    // Read it back
    let read = fs::read(&path)?;
    assert_eq!(read.len(), data.len());

    // Clean up
    let _ = fs::remove_file(&path);

    eprintln!("IO work complete. Duration: {:?}", start.elapsed());
    Ok(())
}

fn json_read(path: Option<&str>) -> anyhow::Result<()> {
    let start = Instant::now();
    let content = if let Some(p) = path {
        fs::read_to_string(p)?
    } else {
        // Default small JSON
        r#"{"foo": "bar", "count": 123, "active": true}"#.to_string()
    };

    let _val: serde_json::Value = serde_json::from_str(&content)?;
    eprintln!("JSON work complete. Duration: {:?}", start.elapsed());
    Ok(())
}

fn cli_compare(baseline: &str, current: &str) -> anyhow::Result<()> {
    with_tempdir(|out_dir| {
        let out = out_dir.join("out.json");
        run_perfgate_policy_ok([
            "compare",
            "--baseline",
            baseline,
            "--current",
            current,
            "--out",
            path_arg(&out).as_str(),
        ])
    })
}

fn cli_check(bench: &str) -> anyhow::Result<()> {
    with_tempdir(|out_dir| {
        run_perfgate_policy_ok([
            "check",
            "--config",
            ".ci/fixtures/check/perfgate.toml",
            "--bench",
            bench,
            "--out-dir",
            path_arg(out_dir).as_str(),
        ])
    })
}

fn render(command: &str, output_file: &str) -> anyhow::Result<()> {
    with_tempdir(|out_dir| {
        let out = out_dir.join(output_file);
        run_perfgate([
            command,
            "--compare",
            ".ci/fixtures/compare/compare-receipt.json",
            "--out",
            path_arg(&out).as_str(),
        ])
    })
}

fn with_tempdir<T>(f: impl FnOnce(&Path) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let dir = unique_tempdir();
    fs::create_dir_all(&dir)?;
    let result = f(&dir);
    let cleanup = fs::remove_dir_all(&dir);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(err)) => Err(err.into()),
        (Err(err), _) => Err(err),
    }
}

fn unique_tempdir() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    env::temp_dir().join(format!("perfgate-selfbench-{}-{now}", std::process::id()))
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn run_perfgate_policy_ok<const N: usize>(args: [&str; N]) -> anyhow::Result<()> {
    let status = run_perfgate_status(args)?;
    match status.code() {
        Some(0 | 2 | 3) => Ok(()),
        _ => anyhow::bail!("perfgate exited with status {status}"),
    }
}

fn run_perfgate<const N: usize>(args: [&str; N]) -> anyhow::Result<()> {
    let status = run_perfgate_status(args)?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("perfgate exited with status {status}")
    }
}

fn run_perfgate_status<const N: usize>(
    args: [&str; N],
) -> anyhow::Result<std::process::ExitStatus> {
    let bin = perfgate_bin()?;
    Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .status()
        .map_err(Into::into)
}

fn perfgate_bin() -> anyhow::Result<PathBuf> {
    let exe = env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve perfgate-selfbench directory"))?;
    let name = if cfg!(windows) {
        "perfgate.exe"
    } else {
        "perfgate"
    };
    let sibling = dir.join(name);
    if sibling.is_file() {
        return Ok(sibling);
    }

    let fallback = PathBuf::from("target").join("release").join(name);
    if fallback.is_file() {
        return Ok(fallback);
    }

    if let Some(path_bin) = find_on_path(name) {
        return Ok(path_bin);
    }

    anyhow::bail!(
        "perfgate binary not found; build it with `cargo build --release -p perfgate-cli --bin perfgate` or install it on PATH"
    )
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempdir_names_include_process_id() {
        let name = unique_tempdir();
        let rendered = name.to_string_lossy();
        assert!(rendered.contains("perfgate-selfbench-"));
        assert!(rendered.contains(&std::process::id().to_string()));
    }

    #[test]
    fn path_arg_preserves_file_name() {
        let path = PathBuf::from("target/release/perfgate");
        assert!(path_arg(&path).ends_with("perfgate"));
    }

    #[test]
    fn find_on_path_returns_none_for_missing_binary() {
        assert!(find_on_path("definitely-not-a-perfgate-binary").is_none());
    }
}
