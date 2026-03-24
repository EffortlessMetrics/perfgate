use anyhow::Context;
use perfgate_types::{CompareReceipt, Metric, MetricStatus};
use std::fs;
use std::path::PathBuf;

pub struct ExplainRequest {
    pub compare: PathBuf,
}

pub struct ExplainOutcome {
    pub markdown: String,
}

pub struct ExplainUseCase;

impl ExplainUseCase {
    pub fn execute(&self, req: ExplainRequest) -> anyhow::Result<ExplainOutcome> {
        let content = fs::read_to_string(&req.compare)
            .with_context(|| format!("failed to read {:?}", req.compare))?;
        let compare: CompareReceipt =
            serde_json::from_str(&content).context("failed to parse JSON")?;

        let mut md = String::new();
        md.push_str(&format!(
            "# Performance Analysis for `{}`\n\n",
            compare.bench.name
        ));

        if compare.verdict.status == perfgate_types::VerdictStatus::Pass {
            md.push_str("✅ **Great news!** No significant performance regressions were detected in this run.\n");
            return Ok(ExplainOutcome { markdown: md });
        }

        md.push_str("⚠️ **Performance Regressions Detected**\n\n");
        md.push_str("The following metrics exceeded their budgets. Below are automated playbooks to help diagnose and resolve the issues:\n\n");

        for (metric, delta) in &compare.deltas {
            if delta.status == MetricStatus::Fail || delta.status == MetricStatus::Warn {
                let threshold = compare
                    .budgets
                    .get(metric)
                    .map(|b| {
                        if delta.status == MetricStatus::Fail {
                            b.threshold
                        } else {
                            b.warn_threshold
                        }
                    })
                    .unwrap_or(0.0);
                md.push_str(&format!("## {}\n", metric.as_str()));
                md.push_str(&format!(
                    "**Regression**: {:.2}% (Threshold: {:.2}%)\n\n",
                    delta.regression * 100.0,
                    threshold * 100.0
                ));
                md.push_str(&Self::playbook_for_metric(metric));
                md.push('\n');
            }
        }

        md.push_str("---\n\n");
        md.push_str("### 🤖 LLM Prompt\n");
        md.push_str("Copy the text below and paste it into an LLM (like Gemini, ChatGPT, or Claude) along with your PR diff to get a detailed explanation:\n\n");
        md.push_str("```text\n");
        md.push_str("Act as a senior performance engineer. I have a performance regression in my pull request.\n\n");
        md.push_str(&format!("Benchmark: {}\n", compare.bench.name));
        for (metric, delta) in &compare.deltas {
            if delta.status == MetricStatus::Fail || delta.status == MetricStatus::Warn {
                md.push_str(&format!(
                    "- {} degraded by {:.2}%\n",
                    metric.as_str(),
                    delta.regression * 100.0
                ));
            }
        }
        md.push_str("\nPlease analyze the attached code diff and explain what changes might have caused these specific metric regressions. Suggest code optimizations to fix the issue.\n");
        md.push_str("```\n");

        Ok(ExplainOutcome { markdown: md })
    }

    fn playbook_for_metric(metric: &Metric) -> String {
        match metric {
            Metric::WallMs => "### Wall Time Playbook\n- **Check for blocking I/O**: Are you doing disk or network operations on the main thread?\n- **Algorithm Complexity**: Did you add nested loops or expensive sorts?\n- **Lock Contention**: Check for deadlocks or heavy mutex usage in concurrent code.".to_string(),
            Metric::CpuMs => "### CPU Time Playbook\n- **Hot Loops**: Profile the code (e.g. using `perf` or `flamegraph`) to find where CPU time is spent.\n- **Allocation Overhead**: Did you add unnecessary clones or heap allocations inside a loop?\n- **Inlining**: Ensure small, frequently called functions are inlined.".to_string(),
            Metric::MaxRssKb => "### Peak Memory (RSS) Playbook\n- **Memory Leaks**: Check if you are retaining references to objects that should be dropped.\n- **Buffer Sizing**: Are you pre-allocating extremely large buffers? Consider streaming or chunking data.\n- **Data Structures**: Can you use more memory-efficient data structures (e.g. `Box<[T]>` instead of `Vec<T>`)?".to_string(),
            Metric::IoReadBytes => "### Disk Read Playbook\n- **Redundant Reads**: Are you reading the same file multiple times?\n- **Buffering**: Use buffered readers (`BufReader`) to reduce syscalls.\n- **Lazy Loading**: Delay reading file contents until strictly necessary.".to_string(),
            Metric::IoWriteBytes => "### Disk Write Playbook\n- **Redundant Writes**: Can you batch writes in memory before flushing to disk?\n- **Buffering**: Use `BufWriter` for many small writes.\n- **Log Level**: Did you accidentally leave verbose logging enabled?".to_string(),
            Metric::NetworkPackets => "### Network Playbook\n- **Batching**: Are you making N+1 API queries? Consolidate them into a bulk endpoint.\n- **Caching**: Cache immutable remote resources instead of fetching them repeatedly.\n- **Connection Pooling**: Are you opening a new TCP connection for every request? Reuse connections.".to_string(),
            Metric::CtxSwitches => "### Context Switch Playbook\n- **Thread Thrashing**: Are you spawning too many threads for CPU-bound work? (Match thread count to physical cores).\n- **Async Yielding**: Are you yielding too often in an async executor?\n- **Lock Contention**: High context switches often point to threads repeatedly waking up and going back to sleep on a lock.".to_string(),
            Metric::PageFaults => "### Page Faults Playbook\n- **Memory Thrashing**: You might be allocating memory faster than the OS can provide physical pages. Pre-allocate and reuse memory buffers.\n- **Memory Mapping**: If using `mmap`, sequential access is better than random access for triggering pre-fetching.".to_string(),
            Metric::BinaryBytes => "### Binary Size Playbook\n- **Dependency Bloat**: Run `cargo tree` to see if a heavy dependency was introduced. Use the perfgate Binary Blame feature.\n- **Monomorphization**: Heavy use of generics can lead to code bloat. Try using trait objects (`dyn Trait`) in cold paths.\n- **Debug Info**: Ensure you are stripping debug symbols in release builds.".to_string(),
            Metric::ThroughputPerS => "### Throughput Playbook\n- **Bottlenecks**: A drop in throughput usually indicates a bottleneck in CPU or I/O. Consult the Wall Time and CPU playbooks.\n- **Concurrency Limit**: Check if a semaphore or connection pool is artificially limiting concurrent work units.".to_string(),
            Metric::EnergyUj => "### Energy Efficiency Playbook\n- **Busy Waiting**: Are you using `spin` loops? Use OS-backed blocking primitives instead.\n- **High CPU Utilization**: Energy correlates strongly with CPU time. Optimize your algorithms to do less work.\n- **Polling**: Switch from polling models to event-driven (interrupt-based) architectures.".to_string(),
        }
    }
}
