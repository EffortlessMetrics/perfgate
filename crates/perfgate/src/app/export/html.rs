//! HTML export rendering.

use std::fmt::Write;

use super::escape::html_escape;
use super::{CompareExportRow, RunExportRow};

pub(super) fn run_row_to_html(row: &RunExportRow) -> anyhow::Result<String> {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>perfgate run export</title></head><body>\
         <h1>perfgate run export</h1>\
         <table border=\"1\">\
         <thead><tr><th>bench_name</th><th>wall_ms_median</th><th>wall_ms_min</th><th>wall_ms_max</th><th>binary_bytes_median</th><th>cpu_ms_median</th><th>ctx_switches_median</th><th>max_rss_kb_median</th><th>page_faults_median</th><th>io_read_bytes_median</th><th>io_write_bytes_median</th><th>network_packets_median</th><th>energy_uj_median</th><th>throughput_median</th><th>sample_count</th><th>timestamp</th></tr></thead>\
         <tbody><tr><td>{bench}</td><td>{wall_med}</td><td>{wall_min}</td><td>{wall_max}</td><td>{binary}</td><td>{cpu}</td><td>{ctx}</td><td>{rss}</td><td>{pf}</td><td>{io_read}</td><td>{io_write}</td><td>{net}</td><td>{energy}</td><td>{throughput}</td><td>{sample_count}</td><td>{timestamp}</td></tr></tbody>\
         </table></body></html>\n",
        bench = html_escape(&row.bench_name),
        wall_med = row.wall_ms_median,
        wall_min = row.wall_ms_min,
        wall_max = row.wall_ms_max,
        binary = row
            .binary_bytes_median
            .map_or(String::new(), |v| v.to_string()),
        cpu = row.cpu_ms_median.map_or(String::new(), |v| v.to_string()),
        ctx = row
            .ctx_switches_median
            .map_or(String::new(), |v| v.to_string()),
        rss = row
            .max_rss_kb_median
            .map_or(String::new(), |v| v.to_string()),
        pf = row
            .page_faults_median
            .map_or(String::new(), |v| v.to_string()),
        io_read = row
            .io_read_bytes_median
            .map_or(String::new(), |v| v.to_string()),
        io_write = row
            .io_write_bytes_median
            .map_or(String::new(), |v| v.to_string()),
        net = row
            .network_packets_median
            .map_or(String::new(), |v| v.to_string()),
        energy = row
            .energy_uj_median
            .map_or(String::new(), |v| v.to_string()),
        throughput = row
            .throughput_median
            .map_or(String::new(), |v| format!("{v:.6}")),
        sample_count = row.sample_count,
        timestamp = html_escape(&row.timestamp),
    );
    Ok(html)
}

pub(super) fn compare_rows_to_html(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut out = String::from(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>perfgate compare export</title></head><body><h1>perfgate compare export</h1><table border=\"1\"><thead><tr><th>bench_name</th><th>metric</th><th>baseline_value</th><th>current_value</th><th>regression_pct</th><th>status</th><th>threshold</th></tr></thead><tbody>",
    );

    for row in rows {
        write!(
            out,
            "<tr><td>{}</td><td>{}</td><td>{:.6}</td><td>{:.6}</td><td>{:.6}</td><td>{}</td><td>{:.6}</td></tr>",
            html_escape(&row.bench_name),
            html_escape(&row.metric),
            row.baseline_value,
            row.current_value,
            row.regression_pct,
            html_escape(&row.status),
            row.threshold
        )?;
    }

    out.push_str("</tbody></table></body></html>\n");
    Ok(out)
}
