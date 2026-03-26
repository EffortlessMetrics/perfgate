//! Test utilities and fake implementations for perfgate testing.
//!
//! This crate provides deterministic, configurable test doubles for the
//! perfgate adapter traits. Use these in unit tests and integration tests
//! to avoid I/O and ensure reproducible test results.
//!
//! # Available Fakes
//!
//! - [`FakeProcessRunner`] - Configurable process runner for testing
//! - [`FakeHostProbe`] - Configurable host probe for testing
//! - [`FakeClock`] - Configurable clock for time-based testing
//! - [`MockProcessBuilder`] - Builder pattern for creating mock process results
//!
//! # Example
//!
//! ```
//! use perfgate_fake::{FakeProcessRunner, MockProcessBuilder};
//! use perfgate_adapters::{ProcessRunner, CommandSpec, RunResult};
//!
//! let runner = FakeProcessRunner::new();
//!
//! // Configure a result using the builder
//! let result = MockProcessBuilder::new()
//!     .exit_code(0)
//!     .wall_ms(100)
//!     .stdout(b"hello world".to_vec())
//!     .build();
//!
//! runner.set_result(&["echo", "hello"], result);
//!
//! // Now when we run the command, we get our configured result
//! let spec = CommandSpec {
//!     name: "echo test".to_string(),
//!     argv: vec!["echo".to_string(), "hello".to_string()],
//!     cwd: None,
//!     env: vec![],
//!     timeout: None,
//!     output_cap_bytes: 1024,
//! };
//!
//! let output = runner.run(&spec).unwrap();
//! assert_eq!(output.exit_code, 0);
//! assert_eq!(output.wall_ms, 100);
//! ```

mod builder;
mod clock;
mod host_probe;
mod process_runner;

pub use builder::MockProcessBuilder;
pub use clock::FakeClock;
pub use host_probe::FakeHostProbe;
pub use process_runner::FakeProcessRunner;

pub use perfgate_adapters::{
    AdapterError, CommandSpec, HostProbeOptions, ProcessRunner, RunResult,
};
