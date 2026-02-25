//! Fake process runner for deterministic testing.

use crate::{AdapterError, CommandSpec, ProcessRunner, RunResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A process runner that returns pre-configured results for specific commands.
///
/// This is useful for testing code that depends on [`ProcessRunner`] without
/// actually spawning processes. Results can be configured per-command or
/// using a fallback.
///
/// # Thread Safety
///
/// All configuration methods are `&self` (not `&mut self`), making it safe
/// to share a single instance across multiple threads in tests.
///
/// # Example
///
/// ```
/// use perfgate_fake::FakeProcessRunner;
/// use perfgate_adapters::{ProcessRunner, CommandSpec, RunResult};
///
/// let runner = FakeProcessRunner::new();
///
/// // Configure a result for a specific command
/// runner.set_result(
///     &["echo", "hello"],
///     RunResult {
///         wall_ms: 50,
///         exit_code: 0,
///         timed_out: false,
///         cpu_ms: Some(10),
///         page_faults: None,
///         ctx_switches: None,
///         max_rss_kb: Some(1024),
///         binary_bytes: None,
///         stdout: b"hello\n".to_vec(),
///         stderr: vec![],
///     },
/// );
///
/// // Get the history of executed commands
/// assert!(runner.history().is_empty());
/// ```
#[derive(Debug, Default, Clone)]
pub struct FakeProcessRunner {
    results: Arc<Mutex<HashMap<String, RunResult>>>,
    fallback: Arc<Mutex<Option<RunResult>>>,
    history: Arc<Mutex<Vec<CommandSpec>>>,
}

impl FakeProcessRunner {
    /// Create a new `FakeProcessRunner` with no configured results.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure a result for a specific command argv.
    ///
    /// The argv is joined with spaces to create a lookup key.
    /// When a matching command is run, the configured result is returned.
    pub fn set_result(&self, argv: &[&str], result: RunResult) {
        let key = argv.join(" ");
        self.results.lock().expect("lock").insert(key, result);
    }

    /// Configure a fallback result for any command without a specific result.
    ///
    /// This is useful for tests that don't care about the exact command
    /// but need some default behavior.
    pub fn set_fallback(&self, result: RunResult) {
        *self.fallback.lock().expect("lock") = Some(result);
    }

    /// Get the history of executed commands.
    ///
    /// Commands are recorded in the order they were run.
    pub fn history(&self) -> Vec<CommandSpec> {
        self.history.lock().expect("lock").clone()
    }

    /// Clear all configured results and history.
    pub fn clear(&self) {
        self.results.lock().expect("lock").clear();
        *self.fallback.lock().expect("lock") = None;
        self.history.lock().expect("lock").clear();
    }

    /// Get the number of times any command has been run.
    pub fn call_count(&self) -> usize {
        self.history.lock().expect("lock").len()
    }

    /// Check if a specific command was run.
    pub fn was_run(&self, argv: &[&str]) -> bool {
        let key = argv.join(" ");
        self.history
            .lock()
            .expect("lock")
            .iter()
            .any(|spec| spec.argv.join(" ") == key)
    }

    /// Get the nth command that was run (0-indexed).
    pub fn nth_call(&self, n: usize) -> Option<CommandSpec> {
        self.history.lock().expect("lock").get(n).cloned()
    }
}

impl ProcessRunner for FakeProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError> {
        self.history.lock().expect("lock").push(spec.clone());

        let key = spec.argv.join(" ");
        let results = self.results.lock().expect("lock");
        if let Some(res) = results.get(&key) {
            return Ok(res.clone());
        }

        let fallback = self.fallback.lock().expect("lock");
        if let Some(res) = &*fallback {
            return Ok(res.clone());
        }

        Err(AdapterError::Other(anyhow::anyhow!(
            "FakeProcessRunner: no result configured for command: {:?}",
            spec.argv
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(exit_code: i32, wall_ms: u64) -> RunResult {
        RunResult {
            wall_ms,
            exit_code,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            binary_bytes: None,
            stdout: vec![],
            stderr: vec![],
        }
    }

    fn make_spec(argv: Vec<&str>) -> CommandSpec {
        CommandSpec {
            argv: argv.into_iter().map(String::from).collect(),
            cwd: None,
            env: vec![],
            timeout: None,
            output_cap_bytes: 1024,
        }
    }

    #[test]
    fn new_runner_is_empty() {
        let runner = FakeProcessRunner::new();
        assert!(runner.history().is_empty());
        assert_eq!(runner.call_count(), 0);
    }

    #[test]
    fn set_result_returns_configured() {
        let runner = FakeProcessRunner::new();
        runner.set_result(&["echo", "hello"], make_result(0, 50));

        let result = runner.run(&make_spec(vec!["echo", "hello"])).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.wall_ms, 50);
    }

    #[test]
    fn fallback_is_used_when_no_match() {
        let runner = FakeProcessRunner::new();
        runner.set_fallback(make_result(42, 100));

        let result = runner.run(&make_spec(vec!["unknown"])).unwrap();
        assert_eq!(result.exit_code, 42);
        assert_eq!(result.wall_ms, 100);
    }

    #[test]
    fn specific_result_takes_precedence_over_fallback() {
        let runner = FakeProcessRunner::new();
        runner.set_result(&["echo"], make_result(0, 10));
        runner.set_fallback(make_result(1, 999));

        let result = runner.run(&make_spec(vec!["echo"])).unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn error_when_no_result_configured() {
        let runner = FakeProcessRunner::new();
        let result = runner.run(&make_spec(vec!["unknown"]));
        assert!(result.is_err());
    }

    #[test]
    fn history_records_commands() {
        let runner = FakeProcessRunner::new();
        runner.set_fallback(make_result(0, 0));

        runner.run(&make_spec(vec!["cmd1"])).unwrap();
        runner.run(&make_spec(vec!["cmd2", "arg"])).unwrap();

        let history = runner.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].argv, vec!["cmd1"]);
        assert_eq!(history[1].argv, vec!["cmd2", "arg"]);
    }

    #[test]
    fn was_run_checks_history() {
        let runner = FakeProcessRunner::new();
        runner.set_fallback(make_result(0, 0));

        assert!(!runner.was_run(&["echo"]));

        runner.run(&make_spec(vec!["echo", "hello"])).unwrap();

        assert!(runner.was_run(&["echo", "hello"]));
        assert!(!runner.was_run(&["echo", "goodbye"]));
    }

    #[test]
    fn nth_call_returns_correct_command() {
        let runner = FakeProcessRunner::new();
        runner.set_fallback(make_result(0, 0));

        runner.run(&make_spec(vec!["first"])).unwrap();
        runner.run(&make_spec(vec!["second"])).unwrap();

        assert_eq!(runner.nth_call(0).unwrap().argv, vec!["first"]);
        assert_eq!(runner.nth_call(1).unwrap().argv, vec!["second"]);
        assert!(runner.nth_call(2).is_none());
    }

    #[test]
    fn clear_resets_everything() {
        let runner = FakeProcessRunner::new();
        runner.set_result(&["cmd"], make_result(0, 0));
        runner.set_fallback(make_result(1, 1));
        runner.run(&make_spec(vec!["cmd"])).unwrap();

        runner.clear();

        assert!(runner.history().is_empty());
        assert!(runner.run(&make_spec(vec!["cmd"])).is_err());
    }

    #[test]
    fn thread_safe_sharing() {
        use std::sync::Arc;
        use std::thread;

        let runner = Arc::new(FakeProcessRunner::new());
        runner.set_fallback(make_result(0, 0));

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let r = runner.clone();
                thread::spawn(move || {
                    r.run(&make_spec(vec!["cmd", &i.to_string()])).unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(runner.call_count(), 4);
    }
}
