use super::{DomainError, summarize_f64, summarize_u64};
use perfgate_types::Stats;

/// Compute perfgate stats from samples.
///
/// Warmup samples (`sample.warmup == true`) are excluded.
///
/// # Examples
///
/// ```
/// use perfgate::domain::compute_stats;
/// use perfgate_types::Sample;
///
/// let samples = vec![
///     Sample {
///         wall_ms: 100, exit_code: 0, warmup: false, timed_out: false,
///         cpu_ms: None, page_faults: None, ctx_switches: None,
///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
///         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
///     },
///     Sample {
///         wall_ms: 120, exit_code: 0, warmup: false, timed_out: false,
///         cpu_ms: None, page_faults: None, ctx_switches: None,
///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
///         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
///     },
/// ];
///
/// let stats = compute_stats(&samples, None).unwrap();
/// assert_eq!(stats.wall_ms.min, 100);
/// assert_eq!(stats.wall_ms.max, 120);
/// ```
#[must_use = "pure computation; call site should use the returned Stats"]
pub fn compute_stats(
    samples: &[perfgate_types::Sample],
    work_units: Option<u64>,
) -> Result<Stats, DomainError> {
    let measured: Vec<&perfgate_types::Sample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() {
        return Err(DomainError::NoSamples);
    }

    let wall: Vec<u64> = measured.iter().map(|s| s.wall_ms).collect();
    let wall_ms = summarize_u64(&wall)?;

    let cpu_vals: Vec<u64> = measured.iter().filter_map(|s| s.cpu_ms).collect();
    let cpu_ms = if cpu_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&cpu_vals)?)
    };

    let page_fault_vals: Vec<u64> = measured.iter().filter_map(|s| s.page_faults).collect();
    let page_faults = if page_fault_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&page_fault_vals)?)
    };

    let ctx_switch_vals: Vec<u64> = measured.iter().filter_map(|s| s.ctx_switches).collect();
    let ctx_switches = if ctx_switch_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&ctx_switch_vals)?)
    };

    let rss_vals: Vec<u64> = measured.iter().filter_map(|s| s.max_rss_kb).collect();
    let max_rss_kb = if rss_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&rss_vals)?)
    };

    let io_read_vals: Vec<u64> = measured.iter().filter_map(|s| s.io_read_bytes).collect();
    let io_read_bytes = if io_read_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&io_read_vals)?)
    };

    let io_write_vals: Vec<u64> = measured.iter().filter_map(|s| s.io_write_bytes).collect();
    let io_write_bytes = if io_write_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&io_write_vals)?)
    };

    let network_vals: Vec<u64> = measured.iter().filter_map(|s| s.network_packets).collect();
    let network_packets = if network_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&network_vals)?)
    };

    let energy_vals: Vec<u64> = measured.iter().filter_map(|s| s.energy_uj).collect();
    let energy_uj = if energy_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&energy_vals)?)
    };

    let binary_vals: Vec<u64> = measured.iter().filter_map(|s| s.binary_bytes).collect();
    let binary_bytes = if binary_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&binary_vals)?)
    };

    let throughput_per_s = match work_units {
        Some(work) => {
            let thr: Vec<f64> = measured
                .iter()
                .map(|s| {
                    let secs = (s.wall_ms as f64) / 1000.0;
                    if secs <= 0.0 {
                        0.0
                    } else {
                        (work as f64) / secs
                    }
                })
                .collect();
            Some(summarize_f64(&thr)?)
        }
        None => None,
    };

    Ok(Stats {
        wall_ms,
        cpu_ms,
        page_faults,
        ctx_switches,
        max_rss_kb,
        io_read_bytes,
        io_write_bytes,
        network_packets,
        energy_uj,
        binary_bytes,
        throughput_per_s,
    })
}
