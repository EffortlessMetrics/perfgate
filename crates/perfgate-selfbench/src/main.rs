use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage_and_exit();
    }

    match args[1].as_str() {
        "noop" => Ok(()),
        "cpu-fixed" => cpu_fixed(),
        "io-fixed" => io_fixed(),
        "json-read" => json_read(args.get(2).map(|s| s.as_str())),
        "perf-wrapper" => {
            let Some(wrapper) = args.get(2) else {
                eprintln!("Usage: perfgate-selfbench perf-wrapper <wrapper>");
                eprintln!(
                    "Wrappers: compare-small, compare-large, check-single, check-no-baseline, render-md, render-report"
                );
                std::process::exit(1);
            };
            perf_wrapper(wrapper)
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage_and_exit();
        }
    }
}

fn print_usage_and_exit() -> ! {
    eprintln!("Usage: perfgate-selfbench <command>");
    eprintln!("Commands: noop, cpu-fixed, io-fixed, json-read, perf-wrapper");
    std::process::exit(1);
}

fn cpu_fixed() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut sum = 0u64;
    // Perform a fixed amount of CPU work
    for i in 0..10_000_000 {
        sum = sum.wrapping_add(i);
    }
    println!("CPU work complete: {}", sum);
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

fn perf_wrapper(wrapper: &str) -> anyhow::Result<()> {
    let perfgate = perfgate_bin()?;
    let temp_dir = TempDir::create()?;

    let mut args: Vec<&str> = match wrapper {
        "compare-small" => vec![
            "compare",
            "--baseline",
            ".ci/fixtures/compare/small-baseline.json",
            "--current",
            ".ci/fixtures/compare/small-current.json",
            "--out",
        ],
        "compare-large" => vec![
            "compare",
            "--baseline",
            ".ci/fixtures/compare/large-baseline.json",
            "--current",
            ".ci/fixtures/compare/large-current.json",
            "--out",
        ],
        "check-single" => vec![
            "check",
            "--config",
            ".ci/fixtures/check/perfgate.toml",
            "--bench",
            "test-bench",
            "--out-dir",
        ],
        "check-no-baseline" => vec![
            "check",
            "--config",
            ".ci/fixtures/check/perfgate.toml",
            "--bench",
            "test-no-baseline",
            "--out-dir",
        ],
        "render-md" => vec![
            "md",
            "--compare",
            ".ci/fixtures/compare/compare-receipt.json",
            "--out",
        ],
        "render-report" => vec![
            "report",
            "--compare",
            ".ci/fixtures/compare/compare-receipt.json",
            "--out",
        ],
        other => anyhow::bail!("unknown perf-wrapper workload: {other}"),
    };

    let output_path;
    if matches!(wrapper, "compare-small" | "compare-large") {
        output_path = temp_dir.path().join("out.json");
        args.push(path_str(&output_path)?);
    } else if matches!(wrapper, "check-single" | "check-no-baseline") {
        args.push(path_str(temp_dir.path())?);
    } else if wrapper == "render-md" {
        output_path = temp_dir.path().join("comment.md");
        args.push(path_str(&output_path)?);
    } else {
        output_path = temp_dir.path().join("report.json");
        args.push(path_str(&output_path)?);
    }

    let status = Command::new(&perfgate)
        .args(args)
        .stdout(Stdio::null())
        .status()?;
    let code = status.code().unwrap_or(1);
    if matches!(
        wrapper,
        "compare-small" | "compare-large" | "check-single" | "check-no-baseline"
    ) && matches!(code, 0 | 2 | 3)
    {
        return Ok(());
    }
    if code == 0 {
        return Ok(());
    }

    std::process::exit(code);
}

fn perfgate_bin() -> anyhow::Result<PathBuf> {
    for candidate in [
        Path::new("./target/release/perfgate"),
        Path::new("./target/release/perfgate.exe"),
    ] {
        if candidate.is_file() {
            return Ok(candidate.to_path_buf());
        }
    }
    if Command::new("perfgate")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        return Ok(PathBuf::from("perfgate"));
    }

    anyhow::bail!("perfgate binary not found in ./target/release or PATH")
}

fn path_str(path: &Path) -> anyhow::Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not valid UTF-8: {}", path.display()))
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn create() -> anyhow::Result<Self> {
        let mut path = env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        path.push(format!("perfgate-selfbench-{}-{nanos}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
