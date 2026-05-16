use std::fmt::Write;

/// Write an optional u64 value to a buffer. Writes nothing if `None`.
pub(super) fn write_opt_u64(buf: &mut String, val: Option<u64>) {
    if let Some(v) = val {
        // write! to a String is infallible, unwrap is safe
        let _ = write!(buf, "{}", v);
    }
}

/// Escape a field for CSV according to RFC 4180.
pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Escape text for HTML/XML contexts.
pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Escape a Prometheus label value.
pub(crate) fn prometheus_escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
