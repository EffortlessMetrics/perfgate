//! Bisection orchestration.

use anyhow::Context;
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

pub struct BisectUseCase;

impl BisectUseCase {
    pub fn execute(&self, req: BisectRequest) -> anyhow::Result<()> {
        let original_branch = Self::get_current_branch()?;

        // 1. Checkout good commit
        println!("Checking out good commit: {}", req.good);
        Self::run_cmd("git", &["checkout", &req.good])?;

        // 2. Build good commit
        println!("Building baseline...");
        Self::run_shell(&req.build_cmd)?;

        // 3. Copy executable to temp
        let baseline_exe = req.executable.with_extension("baseline.exe");
        fs::copy(&req.executable, &baseline_exe).context("Failed to copy baseline executable")?;

        // 4. Start bisection
        println!("Starting git bisect...");
        Self::run_cmd("git", &["bisect", "start", &req.bad, &req.good])?;

        // 5. Loop until bisect finishes
        loop {
            println!("\nBuilding current commit...");
            let build_status = Command::new("sh")
                .arg("-c")
                .arg(&req.build_cmd)
                .status()
                .context("Failed to spawn build command")?;

            let result = if !build_status.success() {
                println!("Build failed, skipping commit...");
                "skip"
            } else {
                println!("Running performance comparison...");
                let mut paired = Command::new("perfgate");
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
                    let author_out = Command::new("git")
                        .args(["show", "-s", "--format='%an <%ae>'", first_word])
                        .output()
                        .ok();
                    if let Some(author_out) = author_out
                        && author_out.status.success()
                    {
                        let author = String::from_utf8_lossy(&author_out.stdout)
                            .trim()
                            .trim_matches('\'')
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
        Self::run_cmd("git", &["bisect", "reset"])?;
        if !original_branch.is_empty() {
            Self::run_cmd("git", &["checkout", &original_branch])?;
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

    fn run_cmd(cmd: &str, args: &[&str]) -> anyhow::Result<()> {
        let status = Command::new(cmd).args(args).status()?;
        if !status.success() {
            anyhow::bail!("Command failed: {} {:?}", cmd, args);
        }
        Ok(())
    }

    fn run_shell(cmd: &str) -> anyhow::Result<()> {
        let status = Command::new("sh").arg("-c").arg(cmd).status()?;
        if !status.success() {
            anyhow::bail!("Shell command failed: {}", cmd);
        }
        Ok(())
    }
}
