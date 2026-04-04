//! Bisection orchestration.

use anyhow::Context;
use perfgate_adapters::{CommandSpec, ProcessRunner, StdProcessRunner};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub struct BisectRequest {
    pub good: String,
    pub bad: String,
    pub build_cmd: String,
    pub executable: PathBuf,
    pub threshold: f64,
}

pub struct BisectUseCase<R: ProcessRunner> {
    runner: R,
}

impl Default for BisectUseCase<StdProcessRunner> {
    fn default() -> Self {
        Self::new(StdProcessRunner)
    }
}

impl<R: ProcessRunner> BisectUseCase<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn execute(&self, req: BisectRequest) -> anyhow::Result<()> {
        let original_branch = Self::get_current_branch()?;

        // 1. Checkout good commit
        println!("Checking out good commit: {}", req.good);
        Self::run_git(&["checkout", &req.good])?;

        // 2. Build good commit
        println!("Building baseline...");
        self.run_shell(&req.build_cmd)?;

        // 3. Copy executable to temp
        let baseline_exe = req.executable.with_extension("baseline.exe");
        fs::copy(&req.executable, &baseline_exe).context("Failed to copy baseline executable")?;

        // 4. Start bisection
        println!("Starting git bisect...");
        Self::run_git(&["bisect", "start", &req.bad, &req.good])?;

        // 5. Loop until bisect finishes
        loop {
            println!("\nBuilding current commit...");
            let build_res = self.run_shell(&req.build_cmd);

            let result = if build_res.is_err() || build_res.unwrap().exit_code != 0 {
                println!("Build failed, skipping commit...");
                "skip"
            } else {
                println!("Running performance comparison...");
                let current_exe = std::env::current_exe()?;
                let mut paired = Command::new(current_exe);
                paired.args([
                    "paired",
                    "--name",
                    "bisect",
                    "--baseline-cmd",
                    &baseline_exe.to_string_lossy(),
                    "--current-cmd",
                    &req.executable.to_string_lossy(),
                    "--fail-on-regression",
                    &req.threshold.to_string(),
                    "--require-significance",
                ]);

                let paired_status = paired.status().context("Failed to run perfgate paired")?;

                if paired_status.success() {
                    println!("Performance looks good!");
                    "good"
                } else {
                    println!("Performance regressed!");
                    "bad"
                }
            };

            let out = Command::new("git")
                .args(["bisect", result])
                .output()
                .context("Failed to run git bisect step")?;
            let stdout = String::from_utf8_lossy(&out.stdout);

            if stdout.contains("is the first bad commit") {
                println!("\n{}", stdout);

                // Regression Blame
                if let Some(first_word) = stdout.split_whitespace().next() {
                    let author_out = match Command::new("git")
                        .args(["show", "-s", "--format=%an <%ae>", first_word])
                        .output()
                    {
                        Ok(out) => Some(out),
                        Err(err) => {
                            eprintln!("warning: git show failed for blame: {err}");
                            None
                        }
                    };
                    if let Some(author_out) = author_out
                        && author_out.status.success()
                    {
                        let author = String::from_utf8_lossy(&author_out.stdout)
                            .trim()
                            .to_string();
                        println!("Regression Blame: Likely introduced by {}", author);
                    }
                }

                break;
            } else if !out.status.success() {
                anyhow::bail!(
                    "git bisect failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }

        // Cleanup
        println!("Cleaning up...");
        let _ = Self::run_git(&["bisect", "reset"]);
        if !original_branch.is_empty() {
            let _ = Self::run_git(&["checkout", &original_branch]);
        }
        let _ = fs::remove_file(&baseline_exe);

        Ok(())
    }

    fn get_current_branch() -> anyhow::Result<String> {
        let out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .context("Failed to get current branch")?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn run_git(args: &[&str]) -> anyhow::Result<()> {
        let status = Command::new("git").args(args).status()?;
        if !status.success() {
            anyhow::bail!("git command failed: {:?}", args);
        }
        Ok(())
    }

    fn run_shell(&self, cmd: &str) -> anyhow::Result<perfgate_adapters::RunResult> {
        let spec = if cfg!(windows) {
            CommandSpec {
                name: "cmd".to_string(),
                argv: vec!["/C".to_string(), cmd.to_string()],
                ..Default::default()
            }
        } else {
            CommandSpec {
                name: "sh".to_string(),
                argv: vec!["-c".to_string(), cmd.to_string()],
                ..Default::default()
            }
        };

        self.runner.run(&spec).map_err(|e| anyhow::anyhow!(e))
    }
}
