//! Paired benchmark execution for perfgate.

use anyhow::Context;
use perfgate_adapters::{CommandSpec, HostProbe, HostProbeOptions, ProcessRunner};
use perfgate_domain::compute_paired_stats;
use perfgate_types::{
    PairedBenchMeta, PairedRunReceipt, PairedSample, PairedSampleHalf, RunMeta, ToolInfo,
    PAIRED_SCHEMA_V1,
};
use std::path::PathBuf;
use std::time::Duration;

use crate::Clock;

#[derive(Debug, Clone)]
pub struct PairedRunRequest {
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub baseline_command: Vec<String>,
    pub current_command: Vec<String>,
    pub repeat: u32,
    pub warmup: u32,
    pub work_units: Option<u64>,
    pub timeout: Option<Duration>,
    pub env: Vec<(String, String)>,
    pub output_cap_bytes: usize,
    pub allow_nonzero: bool,
    pub include_hostname_hash: bool,
}

#[derive(Debug, Clone)]
pub struct PairedRunOutcome {
    pub receipt: PairedRunReceipt,
    pub failed: bool,
    pub reasons: Vec<String>,
}

pub struct PairedRunUseCase<R: ProcessRunner, H: HostProbe, C: Clock> {
    runner: R,
    host_probe: H,
    clock: C,
    tool: ToolInfo,
}

impl<R: ProcessRunner, H: HostProbe, C: Clock> PairedRunUseCase<R, H, C> {
    pub fn new(runner: R, host_probe: H, clock: C, tool: ToolInfo) -> Self {
        Self {
            runner,
            host_probe,
            clock,
            tool,
        }
    }

    pub fn execute(&self, req: PairedRunRequest) -> anyhow::Result<PairedRunOutcome> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let started_at = self.clock.now_rfc3339();
        let host = self.host_probe.probe(&HostProbeOptions {
            include_hostname_hash: req.include_hostname_hash,
        });

        let bench = PairedBenchMeta {
            name: req.name.clone(),
            cwd: req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            baseline_command: req.baseline_command.clone(),
            current_command: req.current_command.clone(),
            repeat: req.repeat,
            warmup: req.warmup,
            work_units: req.work_units,
            timeout_ms: req.timeout.map(|d| d.as_millis() as u64),
        };

        let mut samples = Vec::new();
        let mut reasons = Vec::new();
        let total = req.warmup + req.repeat;

        for i in 0..total {
            let is_warmup = i < req.warmup;

            let baseline_spec = CommandSpec {
                argv: req.baseline_command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };
            let baseline_run = self
                .runner
                .run(&baseline_spec)
                .with_context(|| format!("failed to run baseline (pair {})", i + 1))?;

            let current_spec = CommandSpec {
                argv: req.current_command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };
            let current_run = self
                .runner
                .run(&current_spec)
                .with_context(|| format!("failed to run current (pair {})", i + 1))?;

            let baseline = sample_half(&baseline_run);
            let current = sample_half(&current_run);

            let wall_diff_ms = current.wall_ms as i64 - baseline.wall_ms as i64;
            let rss_diff_kb = match (baseline.max_rss_kb, current.max_rss_kb) {
                (Some(b), Some(c)) => Some(c as i64 - b as i64),
                _ => None,
            };

            if !is_warmup {
                if baseline.timed_out {
                    reasons.push(format!("pair {} baseline timed out", i + 1));
                }
                if baseline.exit_code != 0 {
                    reasons.push(format!(
                        "pair {} baseline exit {}",
                        i + 1,
                        baseline.exit_code
                    ));
                }
                if current.timed_out {
                    reasons.push(format!("pair {} current timed out", i + 1));
                }
                if current.exit_code != 0 {
                    reasons.push(format!("pair {} current exit {}", i + 1, current.exit_code));
                }
            }

            samples.push(PairedSample {
                pair_index: i,
                warmup: is_warmup,
                baseline,
                current,
                wall_diff_ms,
                rss_diff_kb,
            });
        }

        let stats = compute_paired_stats(&samples, req.work_units)?;
        let ended_at = self.clock.now_rfc3339();

        let receipt = PairedRunReceipt {
            schema: PAIRED_SCHEMA_V1.to_string(),
            tool: self.tool.clone(),
            run: RunMeta {
                id: run_id,
                started_at,
                ended_at,
                host,
            },
            bench,
            samples,
            stats,
        };

        let failed = !reasons.is_empty();
        Ok(PairedRunOutcome {
            receipt,
            failed,
            reasons,
        })
    }
}

fn sample_half(run: &perfgate_adapters::RunResult) -> PairedSampleHalf {
    PairedSampleHalf {
        wall_ms: run.wall_ms,
        exit_code: run.exit_code,
        timed_out: run.timed_out,
        max_rss_kb: run.max_rss_kb,
        stdout: if run.stdout.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stdout).to_string())
        },
        stderr: if run.stderr.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stderr).to_string())
        },
    }
}
