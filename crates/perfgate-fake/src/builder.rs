//! Builder pattern for creating mock process results.

use crate::RunResult;

/// Builder for creating mock `RunResult` instances.
///
/// This provides a fluent API for constructing `RunResult` values
/// with sensible defaults for testing.
///
/// # Defaults
///
/// - `wall_ms`: 0
/// - `exit_code`: 0
/// - `timed_out`: false
/// - `cpu_ms`: None
/// - `page_faults`: None
/// - `ctx_switches`: None
/// - `max_rss_kb`: None
/// - `binary_bytes`: None
/// - `stdout`: empty
/// - `stderr`: empty
///
/// # Example
///
/// ```
/// use perfgate_fake::MockProcessBuilder;
///
/// let result = MockProcessBuilder::new()
///     .exit_code(0)
///     .wall_ms(100)
///     .stdout(b"hello world\n".to_vec())
///     .cpu_ms(50)
///     .max_rss_kb(2048)
///     .build();
///
/// assert_eq!(result.exit_code, 0);
/// assert_eq!(result.wall_ms, 100);
/// assert_eq!(result.stdout, b"hello world\n");
/// assert_eq!(result.cpu_ms, Some(50));
/// assert_eq!(result.max_rss_kb, Some(2048));
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockProcessBuilder {
    wall_ms: u64,
    exit_code: i32,
    timed_out: bool,
    cpu_ms: Option<u64>,
    page_faults: Option<u64>,
    ctx_switches: Option<u64>,
    max_rss_kb: Option<u64>,
    binary_bytes: Option<u64>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl MockProcessBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder pre-configured for a successful result.
    ///
    /// Equivalent to `new().exit_code(0)`.
    pub fn success() -> Self {
        Self::new().exit_code(0)
    }

    /// Create a builder pre-configured for a failed result.
    ///
    /// Equivalent to `new().exit_code(1)`.
    pub fn failure() -> Self {
        Self::new().exit_code(1)
    }

    /// Create a builder pre-configured for a timed-out result.
    ///
    /// Equivalent to `new().exit_code(-1).timed_out(true)`.
    pub fn timeout() -> Self {
        Self::new().exit_code(-1).timed_out(true)
    }

    /// Set the wall clock time in milliseconds.
    pub fn wall_ms(mut self, ms: u64) -> Self {
        self.wall_ms = ms;
        self
    }

    /// Set the process exit code.
    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Set whether the process timed out.
    pub fn timed_out(mut self, timed_out: bool) -> Self {
        self.timed_out = timed_out;
        self
    }

    /// Set the CPU time in milliseconds.
    pub fn cpu_ms(mut self, ms: u64) -> Self {
        self.cpu_ms = Some(ms);
        self
    }

    /// Set the number of major page faults.
    pub fn page_faults(mut self, faults: u64) -> Self {
        self.page_faults = Some(faults);
        self
    }

    /// Set the number of context switches.
    pub fn ctx_switches(mut self, switches: u64) -> Self {
        self.ctx_switches = Some(switches);
        self
    }

    /// Set the peak RSS in kilobytes.
    pub fn max_rss_kb(mut self, kb: u64) -> Self {
        self.max_rss_kb = Some(kb);
        self
    }

    /// Set the binary size in bytes.
    pub fn binary_bytes(mut self, bytes: u64) -> Self {
        self.binary_bytes = Some(bytes);
        self
    }

    /// Set the stdout content.
    pub fn stdout(mut self, output: Vec<u8>) -> Self {
        self.stdout = output;
        self
    }

    /// Set the stdout content from a string.
    pub fn stdout_str(mut self, output: &str) -> Self {
        self.stdout = output.as_bytes().to_vec();
        self
    }

    /// Set the stderr content.
    pub fn stderr(mut self, output: Vec<u8>) -> Self {
        self.stderr = output;
        self
    }

    /// Set the stderr content from a string.
    pub fn stderr_str(mut self, output: &str) -> Self {
        self.stderr = output.as_bytes().to_vec();
        self
    }

    /// Build the final `RunResult`.
    pub fn build(self) -> RunResult {
        RunResult {
            wall_ms: self.wall_ms,
            exit_code: self.exit_code,
            timed_out: self.timed_out,
            cpu_ms: self.cpu_ms,
            page_faults: self.page_faults,
            ctx_switches: self.ctx_switches,
            max_rss_kb: self.max_rss_kb,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            binary_bytes: self.binary_bytes,
            stdout: self.stdout,
            stderr: self.stderr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_builder_has_defaults() {
        let result = MockProcessBuilder::new().build();

        assert_eq!(result.wall_ms, 0);
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
        assert!(result.cpu_ms.is_none());
        assert!(result.page_faults.is_none());
        assert!(result.ctx_switches.is_none());
        assert!(result.max_rss_kb.is_none());
        assert!(result.binary_bytes.is_none());
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn success_preset() {
        let result = MockProcessBuilder::success().build();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn failure_preset() {
        let result = MockProcessBuilder::failure().build();
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn timeout_preset() {
        let result = MockProcessBuilder::timeout().build();
        assert_eq!(result.exit_code, -1);
        assert!(result.timed_out);
    }

    #[test]
    fn fluent_configuration() {
        let result = MockProcessBuilder::new()
            .wall_ms(500)
            .exit_code(42)
            .timed_out(false)
            .cpu_ms(200)
            .page_faults(100)
            .ctx_switches(50)
            .max_rss_kb(4096)
            .binary_bytes(8192)
            .stdout(b"out".to_vec())
            .stderr(b"err".to_vec())
            .build();

        assert_eq!(result.wall_ms, 500);
        assert_eq!(result.exit_code, 42);
        assert!(!result.timed_out);
        assert_eq!(result.cpu_ms, Some(200));
        assert_eq!(result.page_faults, Some(100));
        assert_eq!(result.ctx_switches, Some(50));
        assert_eq!(result.max_rss_kb, Some(4096));
        assert_eq!(result.binary_bytes, Some(8192));
        assert_eq!(result.stdout, b"out");
        assert_eq!(result.stderr, b"err");
    }

    #[test]
    fn stdout_str_and_stderr_str() {
        let result = MockProcessBuilder::new()
            .stdout_str("hello\n")
            .stderr_str("warning\n")
            .build();

        assert_eq!(result.stdout, b"hello\n");
        assert_eq!(result.stderr, b"warning\n");
    }

    #[test]
    fn builder_can_be_reused() {
        let base = MockProcessBuilder::new().wall_ms(100);

        let result1 = base.clone().exit_code(0).build();
        let result2 = base.exit_code(1).build();

        assert_eq!(result1.wall_ms, 100);
        assert_eq!(result1.exit_code, 0);
        assert_eq!(result2.wall_ms, 100);
        assert_eq!(result2.exit_code, 1);
    }
}
