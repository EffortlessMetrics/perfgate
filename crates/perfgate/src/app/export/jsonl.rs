//! JSON Lines export rendering.

use std::fmt::Write;

use super::{CompareExportRow, RunExportRow};

/// Format RunExportRow as JSONL.
pub(super) fn run_row_to_jsonl(row: &RunExportRow) -> anyhow::Result<String> {
    let json = serde_json::to_string(row)?;
    let mut out = json;
    out.push('\n');
    Ok(out)
}

/// Format CompareExportRows as JSONL.
pub(super) fn compare_rows_to_jsonl(rows: &[CompareExportRow]) -> anyhow::Result<String> {
    let mut output = String::new();

    for row in rows {
        let json = serde_json::to_string(row)?;
        writeln!(output, "{json}")?;
    }

    Ok(output)
}
