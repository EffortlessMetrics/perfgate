//! Basic example demonstrating host mismatch detection.
//!
//! Run with: cargo run -p perfgate-host-detect --example basic

use perfgate_host_detect::detect_host_mismatch;
use perfgate_types::HostInfo;

fn main() {
    println!("=== perfgate-host-detect Basic Example ===\n");

    println!("1. Identical hosts - no mismatch:");
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: Some(16 * 1024 * 1024 * 1024),
        hostname_hash: Some("abc123".to_string()),
    };
    let current = baseline.clone();

    match detect_host_mismatch(&baseline, &current) {
        None => println!("   No mismatch detected (correct)"),
        Some(m) => println!("   Unexpected mismatch: {:?}", m.reasons),
    }

    println!("\n2. OS mismatch detection:");
    let current_linux = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current_windows = HostInfo {
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    if let Some(m) = detect_host_mismatch(&current_linux, &current_windows) {
        println!("   Detected: {}", m.reasons.join(", "));
    }

    println!("\n3. Architecture mismatch detection:");
    let current_arm = HostInfo {
        os: "linux".to_string(),
        arch: "aarch64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    if let Some(m) = detect_host_mismatch(&current_linux, &current_arm) {
        println!("   Detected: {}", m.reasons.join(", "));
    }

    println!("\n4. CPU count significant difference (> 2x):");
    let small_cpu = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(4),
        memory_bytes: None,
        hostname_hash: None,
    };
    let large_cpu = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(16),
        memory_bytes: None,
        hostname_hash: None,
    };

    if let Some(m) = detect_host_mismatch(&small_cpu, &large_cpu) {
        println!("   Detected: {}", m.reasons.join(", "));
    }

    println!("\n5. Minor CPU difference (<= 2x) - no mismatch:");
    let cpu_8 = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: None,
        hostname_hash: None,
    };
    let cpu_12 = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(12),
        memory_bytes: None,
        hostname_hash: None,
    };

    match detect_host_mismatch(&cpu_8, &cpu_12) {
        None => println!("   No mismatch (8 vs 12 CPUs is within 2x)"),
        Some(m) => println!("   Unexpected: {:?}", m.reasons),
    }

    println!("\n6. Memory significant difference (> 2x):");
    let small_mem = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: Some(8 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };
    let large_mem = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: Some(32 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };

    if let Some(m) = detect_host_mismatch(&small_mem, &large_mem) {
        println!("   Detected: {}", m.reasons.join(", "));
    }

    println!("\n7. Hostname hash difference (different machines):");
    let host_a = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: Some("hash_aaa111".to_string()),
    };
    let host_b = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: Some("hash_bbb222".to_string()),
    };

    if let Some(m) = detect_host_mismatch(&host_a, &host_b) {
        println!("   Detected: {}", m.reasons.join(", "));
    }

    println!("\n8. Multiple simultaneous mismatches:");
    let baseline_full = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(4),
        memory_bytes: Some(8 * 1024 * 1024 * 1024),
        hostname_hash: Some("host1".to_string()),
    };
    let current_full = HostInfo {
        os: "macos".to_string(),
        arch: "aarch64".to_string(),
        cpu_count: Some(16),
        memory_bytes: Some(64 * 1024 * 1024 * 1024),
        hostname_hash: Some("host2".to_string()),
    };

    if let Some(m) = detect_host_mismatch(&baseline_full, &current_full) {
        println!("   Found {} issues:", m.reasons.len());
        for reason in &m.reasons {
            println!("     - {}", reason);
        }
    }

    println!("\n9. Handling missing optional fields:");
    let with_info = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: Some(16 * 1024 * 1024 * 1024),
        hostname_hash: Some("host1".to_string()),
    };
    let without_info = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    match detect_host_mismatch(&with_info, &without_info) {
        None => println!("   No mismatch when optional fields are missing"),
        Some(m) => println!("   Unexpected: {:?}", m.reasons),
    }

    println!("\n=== Example complete ===");
}
