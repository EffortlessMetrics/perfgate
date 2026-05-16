//! CSV export rendering.

use std::fmt::Write;

use super::escape::csv_escape;
use super::{CompareExportRow, RunExportRow};

pub(super) fn run_row_to_csv(row: &RunExportRow) -> anyhow::Result<String> {
    let mut output = String::new();

    output.push_str("bench_name,wall_ms_median,wall_ms_min,wall_ms_max,binary_bytes_median,cpu_ms_median,ctx_switches_median,max_rss_kb_median,page_faults_median,io_read_bytes_median,io_write_bytes_median,network_packets_median,energy_uj_median,throughput_median,sample_count,timestamp\n");

    output.push_str(&csv_escape(&row.bench_name));
    write!(
        output,
        ",{},{},{},",
        row.wall_ms_median, row.wall_ms_min, row.wall_ms_max
    )?;
    write_opt_u64(&mut output, row.binary_bytes_median);
    output.push(',');
    write_opt_u64(&mut output, row.cpu_ms_median);
    output.push(',');
    write_opt_u64(&mut output, row.ctx_switches_median);
    output.push(',');
    write_opt_u64(&mut output, row.max_rss_kb_median);
    output.push(',');
    write_opt_u64(&mut output, row.page_faults_median);
    output.push(',');
    write_opt_u64(&mut output, row.io_read_bytes_median);
    output.push(',');
    write_opt_u64(&mut output, row.io_write_bytes_median);
    output.push(',');
    write_opt_u64(&mut output, row.network_packets_median);
    output.push(',');
    write_opt_u64(&mut output, row.energy_uj_median);
    output.push(',');
    if let Some(v) = row.throughput_median {
        write!(output, "{v:.6}")?;
    }
    write!(output, ",{},", row.sample_count)?;
    output.push_str(&csv_escape(&row.timestamp));
    output.push('\n');

    Ok(output)
}

/// Format CompareExportRows as CSV (RFC 4180).
pub(super) fn compare_rows_to_csv(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut output = String::new();

    output.push_str(
        "bench_name,metric,baseline_value,current_value,regression_pct,status,threshold\n",
    );

    for row in rows {
        output.push_str(&csv_escape(&row.bench_name));
        output.push(',');
        output.push_str(&csv_escape(&row.metric));
        write!(
            output,
            ",{:.6},{:.6},{:.6},",
            row.baseline_value, row.current_value, row.regression_pct
        )?;
        output.push_str(&csv_escape(&row.status));
        writeln!(output, ",{:.6}", row.threshold)?;
    }

    Ok(output)
}

/// Write an optional u64 value to a buffer. Writes nothing if `None`.
fn write_opt_u64(buf: &mut String, val: Option<u64>) {
    if let Some(v) = val {
        // write! to a String is infallible, unwrap is safe
        let _ = write!(buf, "{v}");
    }
}
