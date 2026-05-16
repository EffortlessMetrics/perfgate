//! JUnit XML export rendering.

use std::fmt::Write;

use perfgate_types::RunReceipt;

use super::escape::html_escape;
use super::{CompareExportRow, RunExportRow};
use perfgate_types::CompareReceipt;

pub(super) fn run_row_to_junit_run(
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
        "<testsuites name=\"perfgate\" tests=\"{total}\" failures=\"{failures}\" errors=\"{errors}\">"
    )?;

    writeln!(
        out,
        "  <testsuite name=\"{}\" tests=\"{total}\" failures=\"{failures}\" errors=\"{errors}\">",
        html_escape(&receipt.bench.name)
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
