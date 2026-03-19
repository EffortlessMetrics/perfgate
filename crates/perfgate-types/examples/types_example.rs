//! Demonstrates creating and serializing a RunReceipt.

use perfgate_types::{
    BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
};

fn main() {
    // Build some samples
    let samples: Vec<Sample> = vec![120, 115, 118, 122, 117]
        .into_iter()
        .map(|wall_ms| Sample {
            wall_ms,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            binary_bytes: None,
            stdout: None,
            stderr: None,
        })
        .collect();

    // Summary statistics for wall_ms
    let stats = Stats {
        wall_ms: U64Summary::new(118, 115, 122),
        cpu_ms: None,
        page_faults: None,
        ctx_switches: None,
        max_rss_kb: None,
        binary_bytes: None,
        throughput_per_s: None,
    };

    // Assemble the receipt
    let receipt = RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "run-example-001".to_string(),
            started_at: "2025-01-15T10:00:00Z".to_string(),
            ended_at: "2025-01-15T10:00:05Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(8),
                memory_bytes: Some(16_000_000_000),
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: "my-benchmark".to_string(),
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: 5,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
    };

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&receipt).expect("serialize");
    println!("{json}");

    // Verify round-trip
    let parsed: RunReceipt = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.schema, RUN_SCHEMA_V1);
    assert_eq!(parsed.bench.name, "my-benchmark");
    assert_eq!(parsed.samples.len(), 5);
    println!(
        "\nRound-trip OK: {} samples, median wall_ms = {}",
        parsed.samples.len(),
        parsed.stats.wall_ms.median
    );
}
