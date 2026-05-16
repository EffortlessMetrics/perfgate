//! Prometheus text exposition export rendering.

use std::fmt::Write;

use super::escape::prometheus_escape_label_value;
use super::{CompareExportRow, RunExportRow};

pub(super) fn run_row_to_prometheus(row: &RunExportRow) -> anyhow::Result<String> {
    let bench = prometheus_escape_label_value(&row.bench_name);
    let mut out = String::new();
    writeln!(
        out,
        "perfgate_run_wall_ms_median{{bench=\"{bench}\"}} {}",
        row.wall_ms_median
    )?;
    writeln!(
        out,
        "perfgate_run_wall_ms_min{{bench=\"{bench}\"}} {}",
        row.wall_ms_min
    )?;
    writeln!(
        out,
        "perfgate_run_wall_ms_max{{bench=\"{bench}\"}} {}",
        row.wall_ms_max
    )?;
    write_optional_u64(&mut out, &bench, "binary_bytes", row.binary_bytes_median)?;
    write_optional_u64(&mut out, &bench, "cpu_ms", row.cpu_ms_median)?;
    write_optional_u64(&mut out, &bench, "ctx_switches", row.ctx_switches_median)?;
    write_optional_u64(&mut out, &bench, "max_rss_kb", row.max_rss_kb_median)?;
    write_optional_u64(&mut out, &bench, "page_faults", row.page_faults_median)?;
    write_optional_u64(&mut out, &bench, "io_read_bytes", row.io_read_bytes_median)?;
    write_optional_u64(
        &mut out,
        &bench,
        "io_write_bytes",
        row.io_write_bytes_median,
    )?;
    write_optional_u64(
        &mut out,
        &bench,
        "network_packets",
        row.network_packets_median,
    )?;
    write_optional_u64(&mut out, &bench, "energy_uj", row.energy_uj_median)?;
    if let Some(v) = row.throughput_median {
        writeln!(
            out,
            "perfgate_run_throughput_per_s_median{{bench=\"{bench}\"}} {v:.6}"
        )?;
    }
    writeln!(
        out,
        "perfgate_run_sample_count{{bench=\"{bench}\"}} {}",
        row.sample_count
    )?;
    Ok(out)
}

pub(super) fn compare_rows_to_prometheus(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut out = String::new();
    for row in rows {
        let bench = prometheus_escape_label_value(&row.bench_name);
        let metric = prometheus_escape_label_value(&row.metric);
        writeln!(
            out,
            "perfgate_compare_baseline_value{{bench=\"{bench}\",metric=\"{metric}\"}} {:.6}",
            row.baseline_value
        )?;
        writeln!(
            out,
            "perfgate_compare_current_value{{bench=\"{bench}\",metric=\"{metric}\"}} {:.6}",
            row.current_value
        )?;
        writeln!(
            out,
            "perfgate_compare_regression_pct{{bench=\"{bench}\",metric=\"{metric}\"}} {:.6}",
            row.regression_pct
        )?;
        writeln!(
            out,
            "perfgate_compare_threshold_pct{{bench=\"{bench}\",metric=\"{metric}\"}} {:.6}",
            row.threshold
        )?;

        let status_code = match row.status.as_str() {
            "pass" => 0,
            "warn" => 1,
            "fail" => 2,
            _ => -1,
        };
        writeln!(
            out,
            "perfgate_compare_status{{bench=\"{bench}\",metric=\"{metric}\",status=\"{}\"}} {status_code}",
            prometheus_escape_label_value(&row.status)
        )?;
    }
    Ok(out)
}

fn write_optional_u64(
    out: &mut String,
    bench: &str,
    metric_name: &str,
    value: Option<u64>,
) -> anyhow::Result<()> {
    if let Some(v) = value {
        writeln!(
            out,
            "perfgate_run_{metric_name}_median{{bench=\"{bench}\"}} {v}"
        )?;
    }
    Ok(())
}
