//! Escaping helpers for export renderers.

/// Escape a string for CSV per RFC 4180.
/// If the string contains comma, double quote, or newline, wrap in quotes and escape quotes.
///
/// # Examples
///
/// ```
/// use perfgate::app::export::csv_escape;
///
/// assert_eq!(csv_escape("hello"), "hello");
/// assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
/// assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
/// ```
pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub(super) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn prometheus_escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
