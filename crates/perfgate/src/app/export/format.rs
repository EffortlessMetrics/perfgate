/// Supported export formats.
///
/// # Examples
///
/// ```
/// use perfgate::app::export::ExportFormat;
///
/// let fmt = ExportFormat::Csv;
/// assert_eq!(ExportFormat::parse("csv"), Some(fmt));
/// assert_eq!(ExportFormat::parse("unknown"), None);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// RFC 4180 compliant CSV with header row.
    Csv,
    /// JSON Lines format (one JSON object per line).
    Jsonl,
    /// HTML summary table.
    Html,
    /// Prometheus text exposition format.
    Prometheus,
    /// JUnit XML format (for legacy CI/Jenkins).
    JUnit,
}

impl ExportFormat {
    /// Parse format from string.
    ///
    /// ```
    /// use perfgate::app::export::ExportFormat;
    ///
    /// assert_eq!(ExportFormat::parse("csv"), Some(ExportFormat::Csv));
    /// assert_eq!(ExportFormat::parse("jsonl"), Some(ExportFormat::Jsonl));
    /// assert_eq!(ExportFormat::parse("prometheus"), Some(ExportFormat::Prometheus));
    /// assert_eq!(ExportFormat::parse("unknown"), None);
    /// ```
    pub fn parse(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = ();

    /// Parse an [`ExportFormat`] from a string.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate::app::export::ExportFormat;
    ///
    /// let fmt: ExportFormat = "junit".parse().unwrap();
    /// assert_eq!(fmt, ExportFormat::JUnit);
    ///
    /// let bad: Result<ExportFormat, _> = "nope".parse();
    /// assert!(bad.is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "csv" => Ok(ExportFormat::Csv),
            "jsonl" => Ok(ExportFormat::Jsonl),
            "html" => Ok(ExportFormat::Html),
            "prometheus" | "prom" => Ok(ExportFormat::Prometheus),
            "junit" | "xml" => Ok(ExportFormat::JUnit),
            _ => Err(()),
        }
    }
}
