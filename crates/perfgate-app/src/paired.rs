//! Paired benchmark execution for perfgate.

use anyhow::Context;
use perfgate_adapters::{CommandSpec, HostProbe, HostProbeOptions, ProcessRunner};
use perfgate_domain::compute_paired_stats;
use perfgate_types::{
    PAIRED_SCHEMA_V1, PairedBenchMeta, PairedRunReceipt, PairedSample, PairedSampleHalf, RunMeta,
    ToolInfo,
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
            let baseline_run = self.runner.run(&baseline_spec).map_err(|e| match e {
                perfgate_adapters::AdapterError::RunCommand { command, reason } => {
                    anyhow::anyhow!(
                        "failed to run baseline pair {}: {}: {}",
                        i + 1,
                        command,
                        reason
                    )
                }
                _ => anyhow::anyhow!("failed to run baseline pair {}: {}", i + 1, e),
            })?;

            let current_spec = CommandSpec {
                argv: req.current_command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };
            let current_run = self.runner.run(&current_spec).map_err(|e| match e {
                perfgate_adapters::AdapterError::RunCommand { command, reason } => {
                    anyhow::anyhow!(
                        "failed to run current pair {}: {}: {}",
                        i + 1,
                        command,
                        reason
                    )
                }
                _ => anyhow::anyhow!("failed to run current pair {}: {}", i + 1, e),
            })?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_adapters::{AdapterError, RunResult};
    use perfgate_types::HostInfo;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestRunner {
        runs: Arc<Mutex<Vec<RunResult>>>,
    }

    impl TestRunner {
        fn new(runs: Vec<RunResult>) -> Self {
            Self {
                runs: Arc::new(Mutex::new(runs)),
            }
        }
    }

    impl ProcessRunner for TestRunner {
        fn run(&self, _spec: &CommandSpec) -> Result<RunResult, AdapterError> {
            let mut runs = self.runs.lock().expect("lock runs");
            if runs.is_empty() {
                return Err(AdapterError::Other(anyhow::anyhow!("no more queued runs")));
            }
            Ok(runs.remove(0))
        }
    }

    #[derive(Clone)]
    struct TestHostProbe {
        host: HostInfo,
        seen_include_hash: Arc<Mutex<Vec<bool>>>,
    }

    impl TestHostProbe {
        fn new(host: HostInfo) -> Self {
            Self {
                host,
                seen_include_hash: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl HostProbe for TestHostProbe {
        fn probe(&self, options: &HostProbeOptions) -> HostInfo {
            self.seen_include_hash
                .lock()
                .expect("lock options")
                .push(options.include_hostname_hash);
            self.host.clone()
        }
    }

    #[derive(Clone)]
    struct TestClock {
        now: String,
    }

    impl TestClock {
        fn new(now: &str) -> Self {
            Self {
                now: now.to_string(),
            }
        }
    }

    impl Clock for TestClock {
        fn now_rfc3339(&self) -> String {
            self.now.clone()
        }
    }

    fn run_result(
        wall_ms: u64,
        exit_code: i32,
        timed_out: bool,
        max_rss_kb: Option<u64>,
        stdout: &[u8],
        stderr: &[u8],
    ) -> RunResult {
        RunResult {
            wall_ms,
            exit_code,
            timed_out,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb,
            binary_bytes: None,
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    #[test]
    fn sample_half_maps_optional_output() {
        let run = run_result(10, 0, false, None, b"hello", b"");
        let sample = sample_half(&run);
        assert_eq!(sample.stdout.as_deref(), Some("hello"));
        assert!(sample.stderr.is_none());

        let run2 = run_result(10, 0, false, None, b"", b"err");
        let sample2 = sample_half(&run2);
        assert!(sample2.stdout.is_none());
        assert_eq!(sample2.stderr.as_deref(), Some("err"));
    }

    #[test]
    fn paired_run_collects_samples_and_reasons() {
        let runs = vec![
            // warmup baseline/current (current exits nonzero, should be ignored)
            run_result(100, 0, false, None, b"", b""),
            run_result(90, 1, false, None, b"", b""),
            // measured baseline/current (baseline times out + nonzero)
            run_result(110, 2, true, Some(2000), b"out", b""),
            run_result(105, 0, false, Some(2500), b"", b""),
        ];

        let runner = TestRunner::new(runs);
        let host = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let host_probe = TestHostProbe::new(host.clone());
        let clock = TestClock::new("2024-01-01T00:00:00Z");

        let usecase = PairedRunUseCase::new(
            runner,
            host_probe.clone(),
            clock,
            ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        );

        let outcome = usecase
            .execute(PairedRunRequest {
                name: "bench".to_string(),
                cwd: None,
                baseline_command: vec!["true".to_string()],
                current_command: vec!["true".to_string()],
                repeat: 1,
                warmup: 1,
                work_units: None,
                timeout: None,
                env: vec![],
                output_cap_bytes: 1024,
                allow_nonzero: false,
                include_hostname_hash: true,
            })
            .expect("paired run should succeed");

        assert_eq!(outcome.receipt.samples.len(), 2);
        assert!(outcome.receipt.samples[0].warmup);
        assert!(!outcome.receipt.samples[1].warmup);
        assert_eq!(outcome.receipt.samples[0].pair_index, 0);
        assert_eq!(outcome.receipt.samples[1].pair_index, 1);

        let measured = &outcome.receipt.samples[1];
        assert_eq!(measured.rss_diff_kb, Some(500));

        assert!(outcome.failed);
        assert!(
            outcome
                .reasons
                .iter()
                .any(|r| r.contains("baseline timed out")),
            "expected baseline timeout reason"
        );
        assert!(
            outcome.reasons.iter().any(|r| r.contains("baseline exit")),
            "expected baseline exit reason"
        );
        assert!(
            !outcome
                .reasons
                .iter()
                .any(|r| r.contains("pair 1 current exit")),
            "warmup errors should not be recorded"
        );

        let seen = host_probe.seen_include_hash.lock().expect("lock seen");
        assert_eq!(seen.as_slice(), &[true]);
        assert_eq!(outcome.receipt.run.host, host);
    }

    #[test]
    fn paired_run_all_warmup_no_measured_samples() {
        // 2 warmups, 0 measured → samples has 2 entries, all warmup, no failures
        let runs = vec![
            run_result(100, 0, false, None, b"", b""),
            run_result(90, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(95, 0, false, None, b"", b""),
        ];

        let runner = TestRunner::new(runs);
        let host = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let host_probe = TestHostProbe::new(host);
        let clock = TestClock::new("2024-01-01T00:00:00Z");

        let usecase = PairedRunUseCase::new(
            runner,
            host_probe,
            clock,
            ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        );

        let outcome = usecase
            .execute(PairedRunRequest {
                name: "warmup-only".to_string(),
                cwd: None,
                baseline_command: vec!["true".to_string()],
                current_command: vec!["true".to_string()],
                repeat: 2,
                warmup: 0,
                work_units: None,
                timeout: None,
                env: vec![],
                output_cap_bytes: 1024,
                allow_nonzero: false,
                include_hostname_hash: false,
            })
            .expect("paired run should succeed");

        assert_eq!(outcome.receipt.samples.len(), 2);
        assert!(!outcome.failed);
        assert!(outcome.reasons.is_empty());
    }

    #[test]
    fn paired_run_runner_error_propagates() {
        // Runner that immediately fails
        let runner = TestRunner::new(vec![]);

        let host = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let host_probe = TestHostProbe::new(host);
        let clock = TestClock::new("2024-01-01T00:00:00Z");

        let usecase = PairedRunUseCase::new(
            runner,
            host_probe,
            clock,
            ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        );

        let err = usecase
            .execute(PairedRunRequest {
                name: "fail-bench".to_string(),
                cwd: None,
                baseline_command: vec!["true".to_string()],
                current_command: vec!["true".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout: None,
                env: vec![],
                output_cap_bytes: 1024,
                allow_nonzero: false,
                include_hostname_hash: false,
            })
            .unwrap_err();

        assert!(
            err.to_string().contains("no more queued runs")
                || err.to_string().contains("failed to run"),
            "expected runner error, got: {}",
            err
        );
    }

    #[test]
    fn paired_run_wall_diff_computed_correctly() {
        let runs = vec![
            // baseline: 200ms, current: 150ms → diff = -50
            run_result(200, 0, false, Some(1000), b"", b""),
            run_result(150, 0, false, Some(800), b"", b""),
        ];

        let runner = TestRunner::new(runs);
        let host = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let host_probe = TestHostProbe::new(host);
        let clock = TestClock::new("2024-01-01T00:00:00Z");

        let usecase = PairedRunUseCase::new(
            runner,
            host_probe,
            clock,
            ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
        );

        let outcome = usecase
            .execute(PairedRunRequest {
                name: "diff-bench".to_string(),
                cwd: None,
                baseline_command: vec!["true".to_string()],
                current_command: vec!["true".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout: None,
                env: vec![],
                output_cap_bytes: 1024,
                allow_nonzero: false,
                include_hostname_hash: false,
            })
            .expect("paired run should succeed");

        assert_eq!(outcome.receipt.samples.len(), 1);
        let sample = &outcome.receipt.samples[0];
        assert_eq!(sample.wall_diff_ms, -50);
        assert_eq!(sample.rss_diff_kb, Some(-200));
        assert!(!outcome.failed);
    }
}
