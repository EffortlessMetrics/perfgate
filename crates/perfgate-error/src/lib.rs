//! Unified error types for the perfgate ecosystem.
//!
//! This crate provides a single, comprehensive error type that unifies all error
//! variants from across the perfgate crates. It enables seamless error propagation
//! and conversion between different error types.
//!
//! # Error Categories
//!
//! The [`PerfgateError`] enum is organized into categories:
//!
//! - **Validation**: Bench name validation, config validation
//! - **Stats**: Statistical computation errors (no samples)
//! - **Adapter**: Process execution, I/O, platform-specific errors
//! - **Config**: Configuration parsing and validation errors
//! - **IO**: File system and network I/O errors
//! - **Paired**: Paired benchmark errors
//!
//! # Example
//!
//! ```
//! use perfgate_error::{PerfgateError, ValidationError};
//!
//! fn validate_name(name: &str) -> Result<(), PerfgateError> {
//!     if name.is_empty() {
//!         return Err(ValidationError::Empty.into());
//!     }
//!     Ok(())
//! }
//!
//! let err = validate_name("").unwrap_err();
//! assert!(matches!(err, PerfgateError::Validation(ValidationError::Empty)));
//! ```

use std::fmt;

pub const BENCH_NAME_MAX_LEN: usize = 64;
pub const BENCH_NAME_PATTERN: &str = r"^[a-z0-9_.\-/]+$";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("bench name must not be empty")]
    Empty,

    #[error("bench name {name:?} exceeds maximum length of {max_len} characters")]
    TooLong { name: String, max_len: usize },

    #[error(
        "bench name {name:?} contains invalid characters; \
         allowed: lowercase alphanumeric, dots, underscores, hyphens, slashes"
    )]
    InvalidCharacters { name: String },

    #[error(
        "bench name {name:?} contains an empty path segment \
         (leading, trailing, or consecutive slashes are forbidden)"
    )]
    EmptySegment { name: String },

    #[error(
        "bench name {name:?} contains a {segment:?} path segment (path traversal is forbidden)"
    )]
    PathTraversal { name: String, segment: String },
}

impl ValidationError {
    pub fn name(&self) -> &str {
        match self {
            ValidationError::Empty => "",
            ValidationError::TooLong { name, .. } => name,
            ValidationError::InvalidCharacters { name } => name,
            ValidationError::EmptySegment { name } => name,
            ValidationError::PathTraversal { name, .. } => name,
        }
    }
}

pub fn validate_bench_name(name: &str) -> std::result::Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }
    if name.len() > BENCH_NAME_MAX_LEN {
        return Err(ValidationError::TooLong {
            name: name.to_string(),
            max_len: BENCH_NAME_MAX_LEN,
        });
    }
    if !name.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.' || c == '/' || c == '-'
    }) {
        return Err(ValidationError::InvalidCharacters {
            name: name.to_string(),
        });
    }
    for segment in name.split('/') {
        if segment.is_empty() {
            return Err(ValidationError::EmptySegment {
                name: name.to_string(),
            });
        }
        if segment == "." || segment == ".." {
            return Err(ValidationError::PathTraversal {
                name: name.to_string(),
                segment: segment.to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatsError {
    #[error("no samples to summarize")]
    NoSamples,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairedError {
    #[error("no samples to summarize")]
    NoSamples,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterError {
    #[error("command argv must not be empty")]
    EmptyArgv,

    #[error("command timed out")]
    Timeout,

    #[error("timeout is not supported on this platform")]
    TimeoutUnsupported,

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("bench name validation: {0}")]
    BenchName(String),

    #[error("config validation: {0}")]
    ConfigFile(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IoError {
    #[error("baseline resolve: {0}")]
    BaselineResolve(String),

    #[error("write artifacts: {0}")]
    ArtifactWrite(String),

    #[error("run command: {0}")]
    RunCommand(String),

    #[error("IO error: {0}")]
    Other(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PerfgateError {
    #[error(transparent)]
    Validation(#[from] ValidationError),

    #[error(transparent)]
    Stats(#[from] StatsError),

    #[error(transparent)]
    Adapter(#[from] AdapterError),

    #[error(transparent)]
    Config(#[from] ConfigValidationError),

    #[error(transparent)]
    Io(#[from] IoError),

    #[error(transparent)]
    Paired(#[from] PairedError),
}

impl From<std::io::Error> for PerfgateError {
    fn from(err: std::io::Error) -> Self {
        PerfgateError::Io(IoError::Other(err.to_string()))
    }
}

impl From<std::io::Error> for IoError {
    fn from(err: std::io::Error) -> Self {
        IoError::Other(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    Validation,
    Stats,
    Adapter,
    Config,
    Io,
    Paired,
}

impl PerfgateError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            PerfgateError::Validation(_) => ErrorCategory::Validation,
            PerfgateError::Stats(_) => ErrorCategory::Stats,
            PerfgateError::Adapter(_) => ErrorCategory::Adapter,
            PerfgateError::Config(_) => ErrorCategory::Config,
            PerfgateError::Io(_) => ErrorCategory::Io,
            PerfgateError::Paired(_) => ErrorCategory::Paired,
        }
    }

    pub fn is_recoverable(&self) -> bool {
        match self {
            PerfgateError::Validation(_) => false,
            PerfgateError::Stats(StatsError::NoSamples) => false,
            PerfgateError::Adapter(AdapterError::EmptyArgv) => false,
            PerfgateError::Adapter(AdapterError::Timeout) => true,
            PerfgateError::Adapter(AdapterError::TimeoutUnsupported) => false,
            PerfgateError::Adapter(AdapterError::Other(_)) => true,
            PerfgateError::Config(_) => false,
            PerfgateError::Io(_) => true,
            PerfgateError::Paired(PairedError::NoSamples) => false,
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            PerfgateError::Validation(_) => 1,
            PerfgateError::Stats(_) => 1,
            PerfgateError::Adapter(AdapterError::Timeout) => 1,
            PerfgateError::Adapter(AdapterError::EmptyArgv) => 1,
            PerfgateError::Adapter(AdapterError::TimeoutUnsupported) => 1,
            PerfgateError::Adapter(AdapterError::Other(_)) => 1,
            PerfgateError::Config(_) => 1,
            PerfgateError::Io(_) => 1,
            PerfgateError::Paired(_) => 1,
        }
    }
}

impl ErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCategory::Validation => "validation",
            ErrorCategory::Stats => "stats",
            ErrorCategory::Adapter => "adapter",
            ErrorCategory::Config => "config",
            ErrorCategory::Io => "io",
            ErrorCategory::Paired => "paired",
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub type Result<T> = std::result::Result<T, PerfgateError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_error_empty() {
        let err = ValidationError::Empty;
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn validation_error_too_long() {
        let err = ValidationError::TooLong {
            name: "test".to_string(),
            max_len: 64,
        };
        assert!(err.to_string().contains("exceeds maximum length"));
    }

    #[test]
    fn validation_error_invalid_chars() {
        let err = ValidationError::InvalidCharacters {
            name: "TEST".to_string(),
        };
        assert!(err.to_string().contains("invalid characters"));
    }

    #[test]
    fn validation_error_path_traversal() {
        let err = ValidationError::PathTraversal {
            name: "../test".to_string(),
            segment: "..".to_string(),
        };
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn adapter_error_empty_argv() {
        let err = AdapterError::EmptyArgv;
        assert!(err.to_string().contains("argv"));
    }

    #[test]
    fn adapter_error_timeout() {
        let err = AdapterError::Timeout;
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn adapter_error_timeout_unsupported() {
        let err = AdapterError::TimeoutUnsupported;
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn adapter_error_other() {
        let err = AdapterError::Other("something went wrong".to_string());
        assert!(err.to_string().contains("something went wrong"));
    }

    #[test]
    fn config_validation_error_bench_name() {
        let err = ConfigValidationError::BenchName("invalid name".to_string());
        assert!(err.to_string().contains("bench name"));
    }

    #[test]
    fn config_validation_error_config_file() {
        let err = ConfigValidationError::ConfigFile("missing field".to_string());
        assert!(err.to_string().contains("config"));
    }

    #[test]
    fn io_error_baseline_resolve() {
        let err = IoError::BaselineResolve("file not found".to_string());
        assert!(err.to_string().contains("baseline resolve"));
    }

    #[test]
    fn io_error_artifact_write() {
        let err = IoError::ArtifactWrite("permission denied".to_string());
        assert!(err.to_string().contains("write artifacts"));
    }

    #[test]
    fn io_error_run_command() {
        let err = IoError::RunCommand("spawn failed".to_string());
        assert!(err.to_string().contains("run command"));
    }

    #[test]
    fn perfgate_error_from_validation() {
        let err: PerfgateError = ValidationError::Empty.into();
        assert!(matches!(
            err,
            PerfgateError::Validation(ValidationError::Empty)
        ));
        assert_eq!(err.category(), ErrorCategory::Validation);
    }

    #[test]
    fn perfgate_error_from_stats() {
        let err: PerfgateError = StatsError::NoSamples.into();
        assert!(matches!(err, PerfgateError::Stats(StatsError::NoSamples)));
        assert_eq!(err.category(), ErrorCategory::Stats);
    }

    #[test]
    fn perfgate_error_from_adapter() {
        let err: PerfgateError = AdapterError::Timeout.into();
        assert!(matches!(err, PerfgateError::Adapter(AdapterError::Timeout)));
        assert_eq!(err.category(), ErrorCategory::Adapter);
    }

    #[test]
    fn perfgate_error_from_config() {
        let err: PerfgateError = ConfigValidationError::BenchName("test".to_string()).into();
        assert!(matches!(
            err,
            PerfgateError::Config(ConfigValidationError::BenchName(_))
        ));
        assert_eq!(err.category(), ErrorCategory::Config);
    }

    #[test]
    fn perfgate_error_from_io() {
        let err: PerfgateError = IoError::BaselineResolve("test".to_string()).into();
        assert!(matches!(
            err,
            PerfgateError::Io(IoError::BaselineResolve(_))
        ));
        assert_eq!(err.category(), ErrorCategory::Io);
    }

    #[test]
    fn perfgate_error_from_paired() {
        let err: PerfgateError = PairedError::NoSamples.into();
        assert!(matches!(err, PerfgateError::Paired(PairedError::NoSamples)));
        assert_eq!(err.category(), ErrorCategory::Paired);
    }

    #[test]
    fn error_category_display() {
        assert_eq!(ErrorCategory::Validation.to_string(), "validation");
        assert_eq!(ErrorCategory::Stats.to_string(), "stats");
        assert_eq!(ErrorCategory::Adapter.to_string(), "adapter");
        assert_eq!(ErrorCategory::Config.to_string(), "config");
        assert_eq!(ErrorCategory::Io.to_string(), "io");
        assert_eq!(ErrorCategory::Paired.to_string(), "paired");
    }

    #[test]
    fn is_recoverable_timeout() {
        let err = PerfgateError::Adapter(AdapterError::Timeout);
        assert!(err.is_recoverable());
    }

    #[test]
    fn is_not_recoverable_validation() {
        let err = PerfgateError::Validation(ValidationError::Empty);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn is_not_recoverable_empty_argv() {
        let err = PerfgateError::Adapter(AdapterError::EmptyArgv);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn exit_code_always_positive() {
        let errors: Vec<PerfgateError> = vec![
            ValidationError::Empty.into(),
            StatsError::NoSamples.into(),
            AdapterError::Timeout.into(),
            ConfigValidationError::BenchName("test".to_string()).into(),
            IoError::Other("test".to_string()).into(),
            PairedError::NoSamples.into(),
        ];

        for err in errors {
            assert!(err.exit_code() > 0);
        }
    }

    #[test]
    fn from_std_io_error() {
        let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: PerfgateError = std_err.into();
        assert!(matches!(err, PerfgateError::Io(IoError::Other(_))));
    }

    #[test]
    fn result_type_alias() {
        fn might_fail() -> Result<String> {
            Err(PerfgateError::Validation(ValidationError::Empty))
        }
        let result = might_fail();
        assert!(result.is_err());
    }

    #[test]
    fn validate_bench_name_valid() {
        assert!(validate_bench_name("my-bench").is_ok());
        assert!(validate_bench_name("bench_a").is_ok());
        assert!(validate_bench_name("path/to/bench").is_ok());
        assert!(validate_bench_name("bench.v2").is_ok());
        assert!(validate_bench_name("a").is_ok());
        assert!(validate_bench_name("123").is_ok());
    }

    #[test]
    fn validate_bench_name_invalid() {
        assert!(validate_bench_name("bench|name").is_err());
        assert!(validate_bench_name("").is_err());
        assert!(validate_bench_name("bench name").is_err());
        assert!(validate_bench_name("bench@name").is_err());
    }

    #[test]
    fn validate_bench_name_path_traversal() {
        assert!(validate_bench_name("../bench").is_err());
        assert!(validate_bench_name("bench/../x").is_err());
        assert!(validate_bench_name("./bench").is_err());
        assert!(validate_bench_name("bench/.").is_err());
    }

    #[test]
    fn validate_bench_name_empty_segments() {
        assert!(validate_bench_name("/bench").is_err());
        assert!(validate_bench_name("bench/").is_err());
        assert!(validate_bench_name("bench//x").is_err());
        assert!(validate_bench_name("/").is_err());
    }

    #[test]
    fn validate_bench_name_length_cap() {
        let name_64 = "a".repeat(BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name_64).is_ok());

        let name_65 = "a".repeat(BENCH_NAME_MAX_LEN + 1);
        assert!(validate_bench_name(&name_65).is_err());
    }

    #[test]
    fn validate_bench_name_case() {
        assert!(validate_bench_name("MyBench").is_err());
        assert!(validate_bench_name("BENCH").is_err());
        assert!(validate_bench_name("benchA").is_err());
    }

    #[test]
    fn validation_error_name_accessor() {
        let err = ValidationError::TooLong {
            name: "test".to_string(),
            max_len: 64,
        };
        assert_eq!(err.name(), "test");

        let err = ValidationError::Empty;
        assert_eq!(err.name(), "");
    }

    #[test]
    fn validation_error_empty_segment() {
        let err = ValidationError::EmptySegment {
            name: "bench//x".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("empty path segment"));
        assert!(msg.contains("bench//x"));
    }

    #[test]
    fn stats_error_no_samples_display() {
        let err = StatsError::NoSamples;
        assert_eq!(err.to_string(), "no samples to summarize");
    }

    #[test]
    fn paired_error_no_samples_display() {
        let err = PairedError::NoSamples;
        assert_eq!(err.to_string(), "no samples to summarize");
    }

    #[test]
    fn io_error_other_display() {
        let err = IoError::Other("disk full".to_string());
        assert!(err.to_string().contains("disk full"));
    }

    #[test]
    fn perfgate_error_transparent_display_forwards() {
        let inner = ValidationError::InvalidCharacters {
            name: "MY_BENCH".to_string(),
        };
        let outer: PerfgateError = inner.clone().into();
        assert_eq!(outer.to_string(), inner.to_string());
    }

    #[test]
    fn perfgate_error_transparent_display_stats() {
        let inner = StatsError::NoSamples;
        let outer: PerfgateError = inner.clone().into();
        assert_eq!(outer.to_string(), inner.to_string());
    }

    #[test]
    fn perfgate_error_transparent_display_io() {
        let inner = IoError::BaselineResolve("baselines/bench.json".to_string());
        let outer: PerfgateError = inner.clone().into();
        assert_eq!(outer.to_string(), inner.to_string());
        assert!(outer.to_string().contains("baselines/bench.json"));
    }

    #[test]
    fn validation_display_contains_bench_name() {
        let err = ValidationError::TooLong {
            name: "my-long-bench-name".to_string(),
            max_len: 64,
        };
        assert!(err.to_string().contains("my-long-bench-name"));
        assert!(err.to_string().contains("64"));

        let err = ValidationError::InvalidCharacters {
            name: "BAD_NAME".to_string(),
        };
        assert!(err.to_string().contains("BAD_NAME"));

        let err = ValidationError::PathTraversal {
            name: "foo/../bar".to_string(),
            segment: "..".to_string(),
        };
        assert!(err.to_string().contains("foo/../bar"));
        assert!(err.to_string().contains(".."));
    }

    #[test]
    fn io_error_contains_file_path() {
        let err = IoError::BaselineResolve("baselines/perf.json not found".to_string());
        assert!(err.to_string().contains("baselines/perf.json"));

        let err = IoError::ArtifactWrite("artifacts/perfgate/run.json".to_string());
        assert!(err.to_string().contains("artifacts/perfgate/run.json"));

        let err = IoError::RunCommand("failed to spawn /usr/bin/echo".to_string());
        assert!(err.to_string().contains("/usr/bin/echo"));
    }

    #[test]
    fn exit_code_is_always_one() {
        let errors: Vec<PerfgateError> = vec![
            ValidationError::Empty.into(),
            ValidationError::TooLong {
                name: "x".into(),
                max_len: 64,
            }
            .into(),
            ValidationError::InvalidCharacters { name: "X".into() }.into(),
            ValidationError::EmptySegment { name: "/x".into() }.into(),
            ValidationError::PathTraversal {
                name: "..".into(),
                segment: "..".into(),
            }
            .into(),
            StatsError::NoSamples.into(),
            AdapterError::EmptyArgv.into(),
            AdapterError::Timeout.into(),
            AdapterError::TimeoutUnsupported.into(),
            AdapterError::Other("err".into()).into(),
            ConfigValidationError::BenchName("b".into()).into(),
            ConfigValidationError::ConfigFile("c".into()).into(),
            IoError::BaselineResolve("r".into()).into(),
            IoError::ArtifactWrite("w".into()).into(),
            IoError::RunCommand("r".into()).into(),
            IoError::Other("o".into()).into(),
            PairedError::NoSamples.into(),
        ];
        for err in &errors {
            assert_eq!(err.exit_code(), 1, "exit_code for {:?}", err);
        }
    }

    #[test]
    fn error_category_as_str_matches_display() {
        let categories = [
            ErrorCategory::Validation,
            ErrorCategory::Stats,
            ErrorCategory::Adapter,
            ErrorCategory::Config,
            ErrorCategory::Io,
            ErrorCategory::Paired,
        ];
        for cat in &categories {
            assert_eq!(cat.as_str(), cat.to_string());
        }
    }

    #[test]
    fn from_std_io_error_to_io_error() {
        let std_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err: IoError = std_err.into();
        assert!(matches!(err, IoError::Other(_)));
        assert!(err.to_string().contains("access denied"));
    }

    #[test]
    fn is_recoverable_io_errors() {
        let cases = vec![
            PerfgateError::Io(IoError::BaselineResolve("x".into())),
            PerfgateError::Io(IoError::ArtifactWrite("x".into())),
            PerfgateError::Io(IoError::RunCommand("x".into())),
            PerfgateError::Io(IoError::Other("x".into())),
        ];
        for err in cases {
            assert!(
                err.is_recoverable(),
                "IO errors should be recoverable: {:?}",
                err
            );
        }
    }

    #[test]
    fn is_not_recoverable_config_errors() {
        let err = PerfgateError::Config(ConfigValidationError::BenchName("x".into()));
        assert!(!err.is_recoverable());
        let err = PerfgateError::Config(ConfigValidationError::ConfigFile("x".into()));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn is_recoverable_adapter_other() {
        let err = PerfgateError::Adapter(AdapterError::Other("transient".into()));
        assert!(err.is_recoverable());
    }

    #[test]
    fn is_not_recoverable_timeout_unsupported() {
        let err = PerfgateError::Adapter(AdapterError::TimeoutUnsupported);
        assert!(!err.is_recoverable());
    }

    #[test]
    fn is_not_recoverable_paired_no_samples() {
        let err = PerfgateError::Paired(PairedError::NoSamples);
        assert!(!err.is_recoverable());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn error_message_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,50}"
    }

    proptest! {
        #[test]
        fn prop_adapter_error_other_preserves_message(msg in error_message_strategy()) {
            let err = AdapterError::Other(msg.clone());
            let displayed = err.to_string();
            prop_assert!(displayed.contains(&msg));
        }

        #[test]
        fn prop_io_error_other_preserves_message(msg in error_message_strategy()) {
            let err = IoError::Other(msg.clone());
            let displayed = err.to_string();
            prop_assert!(displayed.contains(&msg));
        }

        #[test]
        fn prop_config_error_preserves_message(msg in error_message_strategy()) {
            let err = ConfigValidationError::BenchName(msg.clone());
            let displayed = err.to_string();
            prop_assert!(displayed.contains(&msg));
        }

        #[test]
        fn prop_error_category_consistent(
            msg in error_message_strategy()
        ) {
            let errors: Vec<PerfgateError> = vec![
                PerfgateError::Validation(ValidationError::Empty),
                PerfgateError::Stats(StatsError::NoSamples),
                PerfgateError::Adapter(AdapterError::Other(msg.clone())),
                PerfgateError::Config(ConfigValidationError::ConfigFile(msg.clone())),
                PerfgateError::Io(IoError::Other(msg)),
                PerfgateError::Paired(PairedError::NoSamples),
            ];

            for err in errors {
                let category = err.category();
                let displayed = category.to_string();
                prop_assert!(!displayed.is_empty());
                prop_assert!(err.exit_code() > 0);
            }
        }

        #[test]
        fn prop_validate_bench_name_valid_chars(
            name in "[a-z0-9_.\\-/]{1,64}"
        ) {
            let result = validate_bench_name(&name);
            if !name.contains("..") && !name.contains("./") && !name.starts_with('/') && !name.ends_with('/') && !name.contains("//") {
                let has_invalid = name.split('/').any(|s| s == "." || s == ".." || s.is_empty());
                if !has_invalid {
                    prop_assert!(result.is_ok(), "name '{}' should be valid", name);
                }
            }
        }
    }
}
