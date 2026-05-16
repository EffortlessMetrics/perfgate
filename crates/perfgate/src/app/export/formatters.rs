use std::fmt::Write;

use perfgate_types::{CompareReceipt, RunReceipt};

use super::escape::{csv_escape, html_escape, prometheus_escape_label_value, write_opt_u64};
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
        write!(output, "{:.6}", v)?;
    }
    write!(output, ",{},", row.sample_count)?;
    output.push_str(&csv_escape(&row.timestamp));
    output.push('\n');

    Ok(output)
}

/// Format RunExportRow as JSONL.
pub(super) fn run_row_to_jsonl(row: &RunExportRow) -> anyhow::Result<String> {
    let json = serde_json::to_string(row)?;
    let mut out = json;
    out.push('\n');
    Ok(out)
}

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
            .map_or(String::new(), |v| format!("{:.6}", v)),
        sample_count = row.sample_count,
        timestamp = html_escape(&row.timestamp),
    );
    Ok(html)
}

pub(super) fn run_row_to_prometheus(row: &RunExportRow) -> anyhow::Result<String> {
    let bench = prometheus_escape_label_value(&row.bench_name);
    let mut out = String::new();
    writeln!(
        out,
        "perfgate_run_wall_ms_median{{bench=\"{}\"}} {}",
        bench, row.wall_ms_median
    )?;
    writeln!(
        out,
        "perfgate_run_wall_ms_min{{bench=\"{}\"}} {}",
        bench, row.wall_ms_min
    )?;
    writeln!(
        out,
        "perfgate_run_wall_ms_max{{bench=\"{}\"}} {}",
        bench, row.wall_ms_max
    )?;
    if let Some(v) = row.binary_bytes_median {
        writeln!(
            out,
            "perfgate_run_binary_bytes_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.cpu_ms_median {
        writeln!(
            out,
            "perfgate_run_cpu_ms_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.ctx_switches_median {
        writeln!(
            out,
            "perfgate_run_ctx_switches_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.max_rss_kb_median {
        writeln!(
            out,
            "perfgate_run_max_rss_kb_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.page_faults_median {
        writeln!(
            out,
            "perfgate_run_page_faults_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.io_read_bytes_median {
        writeln!(
            out,
            "perfgate_run_io_read_bytes_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.io_write_bytes_median {
        writeln!(
            out,
            "perfgate_run_io_write_bytes_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.network_packets_median {
        writeln!(
            out,
            "perfgate_run_network_packets_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.energy_uj_median {
        writeln!(
            out,
            "perfgate_run_energy_uj_median{{bench=\"{}\"}} {}",
            bench, v
        )?;
    }
    if let Some(v) = row.throughput_median {
        writeln!(
            out,
            "perfgate_run_throughput_per_s_median{{bench=\"{}\"}} {:.6}",
            bench, v
        )?;
    }
    writeln!(
        out,
        "perfgate_run_sample_count{{bench=\"{}\"}} {}",
        bench, row.sample_count
    )?;
    Ok(out)
}

pub(super) fn run_row_to_junit(
    receipt: &RunReceipt,
    _row: &RunExportRow,
) -> anyhow::Result<String> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<testsuites name=\"perfgate\">\n");
    writeln!(
        out,
        "  <testsuite name=\"{}\" tests=\"1\" failures=\"0\" errors=\"0\">",
        html_escape(&receipt.bench.name)
    )?;
    writeln!(
        out,
        "    <testcase name=\"execution\" classname=\"perfgate.{}\" time=\"{}\">",
        html_escape(&receipt.bench.name),
        receipt.stats.wall_ms.median as f64 / 1000.0
    )?;
    out.push_str("    </testcase>\n");
    out.push_str("  </testsuite>\n");
    out.push_str("</testsuites>\n");
    Ok(out)
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

/// Format CompareExportRows as JSONL.
pub(super) fn compare_rows_to_jsonl(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut output = String::new();

    for row in rows {
        let json = serde_json::to_string(row)?;
        writeln!(output, "{}", json)?;
    }

    Ok(output)
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

pub(super) fn compare_rows_to_junit(
    receipt: &CompareReceipt,
    rows: &[CompareExportRow],
) -> anyhow::Result<String> {
    let mut out = String::new();
    let total = rows.len();
    let failures = rows.iter().filter(|r| r.status == "fail").count();
    let errors = rows.iter().filter(|r| r.status == "error").count();

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    writeln!(
        out,
        "<testsuites name=\"perfgate\" tests=\"{}\" failures=\"{}\" errors=\"{}\">",
        total, failures, errors
    )?;

    writeln!(
        out,
        "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\">",
        html_escape(&receipt.bench.name),
        total,
        failures,
        errors
    )?;

    for row in rows {
        writeln!(
            out,
            "    <testcase name=\"{}\" classname=\"perfgate.{}\" time=\"0.0\">",
            html_escape(&row.metric),
            html_escape(&receipt.bench.name)
        )?;

        if row.status == "fail" {
            write!(
                out,
                "      <failure message=\"Performance regression detected for {}\">",
                html_escape(&row.metric)
            )?;
            write!(
                out,
                "Metric: {}\nBaseline: {:.6}\nCurrent: {:.6}\nRegression: {:.2}%\nThreshold: {:.2}%",
                row.metric,
                row.baseline_value,
                row.current_value,
                row.regression_pct,
                row.threshold
            )?;
            out.push_str("</failure>\n");
        } else if row.status == "error" {
            write!(
                out,
                "      <error message=\"Error occurred during performance check for {}\">",
                html_escape(&row.metric)
            )?;
            out.push_str("</error>\n");
        }

        out.push_str("    </testcase>\n");
    }

    out.push_str("  </testsuite>\n");
    out.push_str("</testsuites>\n");

    Ok(out)
}

pub(super) fn compare_rows_to_prometheus(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut out = String::new();
    for row in rows {
        let bench = prometheus_escape_label_value(&row.bench_name);
        let metric = prometheus_escape_label_value(&row.metric);
        writeln!(
            out,
            "perfgate_compare_baseline_value{{bench=\"{}\",metric=\"{}\"}} {:.6}",
            bench, metric, row.baseline_value
        )?;
        writeln!(
            out,
            "perfgate_compare_current_value{{bench=\"{}\",metric=\"{}\"}} {:.6}",
            bench, metric, row.current_value
        )?;
        writeln!(
            out,
            "perfgate_compare_regression_pct{{bench=\"{}\",metric=\"{}\"}} {:.6}",
            bench, metric, row.regression_pct
        )?;
        writeln!(
            out,
            "perfgate_compare_threshold_pct{{bench=\"{}\",metric=\"{}\"}} {:.6}",
            bench, metric, row.threshold
        )?;

        let status_code = match row.status.as_str() {
            "pass" => 0,
            "warn" => 1,
            "fail" => 2,
            _ => -1,
        };
        writeln!(
            out,
            "perfgate_compare_status{{bench=\"{}\",metric=\"{}\",status=\"{}\"}} {}",
            bench,
            metric,
            prometheus_escape_label_value(&row.status),
            status_code
        )?;
    }
    Ok(out)
}
