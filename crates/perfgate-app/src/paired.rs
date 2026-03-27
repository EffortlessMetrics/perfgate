//! Paired benchmark execution for perfgate.

use perfgate_adapters::{CommandSpec, HostProbe, HostProbeOptions, ProcessRunner};
use perfgate_domain::{compute_paired_cv, compute_paired_stats};
use perfgate_types::{
    NoiseDiagnostics, NoiseLevel, PAIRED_SCHEMA_V1, PairedBenchMeta, PairedRunReceipt,
    PairedSample, PairedSampleHalf, RunMeta, SignificancePolicy, ToolInfo,
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
    pub significance_alpha: Option<f64>,
    pub significance_min_samples: Option<u32>,
    pub require_significance: bool,
    pub max_retries: u32,
    pub fail_on_regression: Option<f64>,
    /// CV threshold for early termination. If the coefficient of variation of
    /// the wall-time differences exceeds this value, retries are aborted because
    /// the benchmark is too noisy for significance to be achievable.
    pub cv_threshold: Option<f64>,
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

        let mut bench = PairedBenchMeta {
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

        // Run warmups first
        for i in 0..req.warmup {
            self.run_pair(i, true, &req, &mut samples, &mut reasons)?;
        }

        // Initial measurement run
        let mut pairs_collected = 0;
        for _ in 0..req.repeat {
            self.run_pair(
                req.warmup + pairs_collected,
                false,
                &req,
                &mut samples,
                &mut reasons,
            )?;
            pairs_collected += 1;
        }

        let significance_policy = SignificancePolicy {
            alpha: req.significance_alpha,
            min_samples: req.significance_min_samples,
        };

        // Retry logic for significance with adaptive sample sizing and CV-based early termination
        let mut retries_done: u32 = 0;
        let mut early_termination = false;
        loop {
            let stats = compute_paired_stats(&samples, req.work_units, Some(&significance_policy))?;
            let significance_reached = stats
                .wall_diff_ms
                .significance
                .as_ref()
                .map(|s| s.significant)
                .unwrap_or(true);

            if !req.require_significance || significance_reached || retries_done >= req.max_retries
            {
                break;
            }

            // Check CV threshold for early termination
            if let Some(cv_thresh) = req.cv_threshold {
                let cv = compute_paired_cv(&samples);
                if cv > cv_thresh {
                    early_termination = true;
                    reasons.push(format!(
                        "early termination: CV {:.3} exceeds threshold {:.3}, benchmark too noisy for retries",
                        cv, cv_thresh
                    ));
                    break;
                }
            }

            // Adaptive sample sizing: each retry collects more pairs (1.5x growth)
            // Retry 1: 2 pairs, Retry 2: 3 pairs, Retry 3: 5 pairs, ...
            let extra_pairs = ((retries_done as f64 + 1.0) * 1.5).ceil() as u32;
            retries_done += 1;

            for _ in 0..extra_pairs {
                self.run_pair(
                    req.warmup + pairs_collected,
                    false,
                    &req,
                    &mut samples,
                    &mut reasons,
                )?;
                pairs_collected += 1;
            }
        }

        // Update bench metadata if we collected more samples than originally requested
        bench.repeat = pairs_collected;

        let stats = compute_paired_stats(&samples, req.work_units, Some(&significance_policy))?;
        let ended_at = self.clock.now_rfc3339();

        // Build noise diagnostics when retries were configured
        let noise_diagnostics = if req.max_retries > 0 {
            let cv = compute_paired_cv(&samples);
            Some(NoiseDiagnostics {
                cv,
                noise_level: NoiseLevel::from_cv(cv),
                retries_used: retries_done,
                early_termination,
            })
        } else {
            None
        };

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
            noise_diagnostics,
        };

        if let Some(threshold_pct) = req.fail_on_regression {
            let comparison = perfgate_domain::compare_paired_stats(&receipt.stats);
            let threshold_fraction = threshold_pct / 100.0;
            if comparison.pct_change > threshold_fraction && comparison.is_significant {
                reasons.push(format!(
                    "wall time regression ({:.2}%) exceeded threshold ({:.2}%)",
                    comparison.pct_change * 100.0,
                    threshold_pct
                ));
            }
        }

        let failed = !reasons.is_empty();
        Ok(PairedRunOutcome {
            receipt,
            failed,
            reasons,
        })
    }

    fn run_pair(
        &self,
        pair_index: u32,
        is_warmup: bool,
        req: &PairedRunRequest,
        samples: &mut Vec<PairedSample>,
        reasons: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let baseline_spec = CommandSpec {
            name: format!("{}-baseline", req.name),
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
                    pair_index + 1,
                    command,
                    reason
                )
            }
            _ => anyhow::anyhow!("failed to run baseline pair {}: {}", pair_index + 1, e),
        })?;

        let current_spec = CommandSpec {
            name: format!("{}-current", req.name),
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
                    pair_index + 1,
                    command,
                    reason
                )
            }
            _ => anyhow::anyhow!("failed to run current pair {}: {}", pair_index + 1, e),
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
                reasons.push(format!("pair {} baseline timed out", pair_index + 1));
            }
            if baseline.exit_code != 0 && !req.allow_nonzero {
                reasons.push(format!(
                    "pair {} baseline exit {}",
                    pair_index + 1,
                    baseline.exit_code
                ));
            }
            if current.timed_out {
                reasons.push(format!("pair {} current timed out", pair_index + 1));
            }
            if current.exit_code != 0 && !req.allow_nonzero {
                reasons.push(format!(
                    "pair {} current exit {}",
                    pair_index + 1,
                    current.exit_code
                ));
            }
        }

        samples.push(PairedSample {
            pair_index,
            warmup: is_warmup,
            baseline,
            current,
            wall_diff_ms,
            rss_diff_kb,
        });

        Ok(())
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
                return Err(AdapterError::Other("no more queued runs".to_string()));
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
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
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
                significance_alpha: None,
                significance_min_samples: None,
                require_significance: false,
                max_retries: 0,
                fail_on_regression: None,
                cv_threshold: None,
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
                significance_alpha: None,
                significance_min_samples: None,
                require_significance: false,
                max_retries: 0,
                fail_on_regression: None,
                cv_threshold: None,
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
                significance_alpha: None,
                significance_min_samples: None,
                require_significance: false,
                max_retries: 0,
                fail_on_regression: None,
                cv_threshold: None,
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
                significance_alpha: None,
                significance_min_samples: None,
                require_significance: false,
                max_retries: 0,
                fail_on_regression: None,
                cv_threshold: None,
            })
            .expect("paired run should succeed");

        assert_eq!(outcome.receipt.samples.len(), 1);
        let sample = &outcome.receipt.samples[0];
        assert_eq!(sample.wall_diff_ms, -50);
        assert_eq!(sample.rss_diff_kb, Some(-200));
        assert!(!outcome.failed);
    }

    #[test]
    fn paired_run_retries_until_significance() {
        // We want to simulate:
        // Initial run (2 pairs): not significant
        // Adaptive retries collect increasing pairs until significance

        // Wall diffs: initially ambiguous, later consistently positive.
        // Retry 1 collects ceil((0+1)*1.5) = 2 pairs, Retry 2 = 3 pairs, etc.
        // Provide enough runs for multiple retries.
        let runs = vec![
            // Pair 1 (diff 0)
            run_result(100, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            // Pair 2 (diff 10)
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            // Retry 1: 2 extra pairs (diff 10 each)
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            // Retry 2: 3 extra pairs (diff 10 each)
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            // Retry 3: 5 extra pairs (diff 10 each) - just in case
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
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
                name: "retry-bench".to_string(),
                cwd: None,
                baseline_command: vec!["true".to_string()],
                current_command: vec!["true".to_string()],
                repeat: 2, // Initial 2 pairs
                warmup: 0,
                work_units: None,
                timeout: None,
                env: vec![],
                output_cap_bytes: 1024,
                allow_nonzero: false,
                include_hostname_hash: false,
                significance_alpha: Some(0.05),
                significance_min_samples: Some(2),
                require_significance: true,
                max_retries: 5, // Allow up to 5 retries
                fail_on_regression: None,
                cv_threshold: None,
            })
            .expect("paired run should succeed");

        // It should have at least 3 samples because it retried
        assert!(outcome.receipt.samples.len() > 2);
        assert_eq!(
            outcome.receipt.bench.repeat,
            outcome.receipt.samples.len() as u32
        );

        // Noise diagnostics should be present because max_retries > 0
        let diag = outcome
            .receipt
            .noise_diagnostics
            .expect("should have noise diagnostics");
        assert!(diag.retries_used > 0);
        assert!(!diag.early_termination);
    }

    #[test]
    fn paired_run_cv_threshold_early_termination() {
        // Simulate very noisy data: large variance in diffs
        // Pair 1: diff = -50, Pair 2: diff = +60 => high CV
        let runs = vec![
            // Pair 1
            run_result(200, 0, false, None, b"", b""),
            run_result(150, 0, false, None, b"", b""),
            // Pair 2
            run_result(100, 0, false, None, b"", b""),
            run_result(160, 0, false, None, b"", b""),
            // Extra pair should NOT be consumed because CV threshold triggers early exit
            // But we provide extras just in case the retry runs one more batch
            run_result(100, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
            run_result(100, 0, false, None, b"", b""),
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
                name: "noisy-bench".to_string(),
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
                significance_alpha: Some(0.05),
                significance_min_samples: Some(2),
                require_significance: true,
                max_retries: 5,
                fail_on_regression: None,
                cv_threshold: Some(0.5),
            })
            .expect("paired run should succeed");

        let diag = outcome
            .receipt
            .noise_diagnostics
            .expect("should have noise diagnostics");
        // Early termination should have kicked in
        assert!(diag.early_termination);
        assert!(diag.cv > 0.5);
        assert_eq!(diag.noise_level, perfgate_types::NoiseLevel::High);

        // Should have a reason about early termination
        assert!(
            outcome
                .reasons
                .iter()
                .any(|r| r.contains("early termination")),
            "expected early termination reason, got: {:?}",
            outcome.reasons
        );
    }

    #[test]
    fn paired_run_no_retries_no_noise_diagnostics() {
        let runs = vec![
            run_result(100, 0, false, None, b"", b""),
            run_result(110, 0, false, None, b"", b""),
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
                name: "simple-bench".to_string(),
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
                significance_alpha: None,
                significance_min_samples: None,
                require_significance: false,
                max_retries: 0,
                fail_on_regression: None,
                cv_threshold: None,
            })
            .expect("paired run should succeed");

        assert!(
            outcome.receipt.noise_diagnostics.is_none(),
            "should not have noise diagnostics when max_retries=0"
        );
    }
}
