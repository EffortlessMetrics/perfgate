//! Fake process runner for deterministic testing.

use crate::{AdapterError, CommandSpec, ProcessRunner, RunResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A process runner that returns pre-configured results for specific commands.
#[derive(Debug, Default, Clone)]
pub struct FakeProcessRunner {
    /// Map from joined command argv to result
    results: Arc<Mutex<HashMap<String, RunResult>>>,
    /// Fallback result if command not found
    fallback: Arc<Mutex<Option<RunResult>>>,
    /// History of executed commands
    history: Arc<Mutex<Vec<CommandSpec>>>,
}

impl FakeProcessRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure a result for a specific command.
    pub fn set_result(&self, argv: &[&str], result: RunResult) {
        let key = argv.join(" ");
        self.results.lock().expect("lock").insert(key, result);
    }

    /// Configure a fallback result.
    pub fn set_fallback(&self, result: RunResult) {
        *self.fallback.lock().expect("lock") = Some(result);
    }

    /// Get history of executed commands.
    pub fn history(&self) -> Vec<CommandSpec> {
        self.history.lock().expect("lock").clone()
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

        Err(AdapterError::Other(format!(
            "FakeProcessRunner: no result configured for command: {:?}",
            spec.argv
        )))
    }
}
