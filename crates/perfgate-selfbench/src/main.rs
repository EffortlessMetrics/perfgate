use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
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
        "perfgate-compare-small" => run_perfgate_workload(Workload::CompareSmall),
        "perfgate-compare-large" => run_perfgate_workload(Workload::CompareLarge),
        "perfgate-check-single" => run_perfgate_workload(Workload::CheckSingle),
        "perfgate-check-no-baseline" => run_perfgate_workload(Workload::CheckNoBaseline),
        "perfgate-render-md" => run_perfgate_workload(Workload::RenderMd),
        "perfgate-render-report" => run_perfgate_workload(Workload::RenderReport),
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
    eprintln!("  noop");
    eprintln!("  cpu-fixed");
    eprintln!("  io-fixed");
    eprintln!("  json-read [path]");
    eprintln!("  perfgate-compare-small");
    eprintln!("  perfgate-compare-large");
    eprintln!("  perfgate-check-single");
    eprintln!("  perfgate-check-no-baseline");
    eprintln!("  perfgate-render-md");
    eprintln!("  perfgate-render-report");
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

#[derive(Clone, Copy)]
enum Workload {
    CompareSmall,
    CompareLarge,
    CheckSingle,
    CheckNoBaseline,
    RenderMd,
    RenderReport,
}

impl Workload {
    fn args(self, out_dir: &Path) -> Vec<String> {
        match self {
            Self::CompareSmall => vec![
                "compare".into(),
                "--baseline".into(),
                ".ci/fixtures/compare/small-baseline.json".into(),
                "--current".into(),
                ".ci/fixtures/compare/small-current.json".into(),
                "--out".into(),
                out_dir.join("out.json").display().to_string(),
            ],
            Self::CompareLarge => vec![
                "compare".into(),
                "--baseline".into(),
                ".ci/fixtures/compare/large-baseline.json".into(),
                "--current".into(),
                ".ci/fixtures/compare/large-current.json".into(),
                "--out".into(),
                out_dir.join("out.json").display().to_string(),
            ],
            Self::CheckSingle => vec![
                "check".into(),
                "--config".into(),
                ".ci/fixtures/check/perfgate.toml".into(),
                "--bench".into(),
                "test-bench".into(),
                "--out-dir".into(),
                out_dir.display().to_string(),
            ],
            Self::CheckNoBaseline => vec![
                "check".into(),
                "--config".into(),
                ".ci/fixtures/check/perfgate.toml".into(),
                "--bench".into(),
                "test-no-baseline".into(),
                "--out-dir".into(),
                out_dir.display().to_string(),
            ],
            Self::RenderMd => vec![
                "md".into(),
                "--compare".into(),
                ".ci/fixtures/compare/compare-receipt.json".into(),
                "--out".into(),
                out_dir.join("comment.md").display().to_string(),
            ],
            Self::RenderReport => vec![
                "report".into(),
                "--compare".into(),
                ".ci/fixtures/compare/compare-receipt.json".into(),
                "--out".into(),
                out_dir.join("report.json").display().to_string(),
            ],
        }
    }

    fn allows_policy_exit(self) -> bool {
        matches!(
            self,
            Self::CompareSmall | Self::CompareLarge | Self::CheckSingle | Self::CheckNoBaseline
        )
    }
}

fn run_perfgate_workload(workload: Workload) -> anyhow::Result<()> {
    let perfgate = perfgate_bin()?;
    let out_dir = TempDir::new("perfgate-selfbench")?;
    let status = Command::new(&perfgate)
        .args(workload.args(out_dir.path()))
        .stdout(std::process::Stdio::null())
        .status()?;

    if status.success() || (workload.allows_policy_exit() && is_policy_exit(status)) {
        Ok(())
    } else {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn is_policy_exit(status: ExitStatus) -> bool {
    matches!(status.code(), Some(2 | 3))
}

fn perfgate_bin() -> anyhow::Result<PathBuf> {
    for candidate in [
        PathBuf::from("./target/release/perfgate"),
        PathBuf::from("./target/release/perfgate.exe"),
    ] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    find_on_path("perfgate").ok_or_else(|| anyhow::anyhow!("perfgate binary not found"))
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }

        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{binary}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> anyhow::Result<Self> {
        let pid = std::process::id();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{pid}-{nanos}"));
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
