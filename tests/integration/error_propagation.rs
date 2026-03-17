//! Integration tests: error propagation across all crates.
//!
//! These tests verify that errors propagate correctly across crate
//! boundaries and that error conversions work as expected.

use perfgate_error::{
    AdapterError, ConfigValidationError, ErrorCategory, IoError, PairedError, PerfgateError,
    StatsError, ValidationError,
};

#[test]
fn validation_error_converts_to_perfgate_error() {
    let err: PerfgateError = ValidationError::Empty.into();
    assert!(matches!(
        err,
        PerfgateError::Validation(ValidationError::Empty)
    ));
    assert_eq!(err.category(), ErrorCategory::Validation);
}

#[test]
fn stats_error_converts_to_perfgate_error() {
    let err: PerfgateError = StatsError::NoSamples.into();
    assert!(matches!(err, PerfgateError::Stats(StatsError::NoSamples)));
    assert_eq!(err.category(), ErrorCategory::Stats);
}

#[test]
fn adapter_error_converts_to_perfgate_error() {
    let err: PerfgateError = AdapterError::Timeout.into();
    assert!(matches!(err, PerfgateError::Adapter(AdapterError::Timeout)));
    assert_eq!(err.category(), ErrorCategory::Adapter);
}

#[test]
fn config_error_converts_to_perfgate_error() {
    let err: PerfgateError = ConfigValidationError::BenchName("test".to_string()).into();
    assert!(matches!(
        err,
        PerfgateError::Config(ConfigValidationError::BenchName(_))
    ));
    assert_eq!(err.category(), ErrorCategory::Config);
}

#[test]
fn io_error_converts_to_perfgate_error() {
    let err: PerfgateError = IoError::BaselineResolve("not found".to_string()).into();
    assert!(matches!(
        err,
        PerfgateError::Io(IoError::BaselineResolve(_))
    ));
    assert_eq!(err.category(), ErrorCategory::Io);
}

#[test]
fn paired_error_converts_to_perfgate_error() {
    let err: PerfgateError = PairedError::NoSamples.into();
    assert!(matches!(err, PerfgateError::Paired(PairedError::NoSamples)));
    assert_eq!(err.category(), ErrorCategory::Paired);
}

#[test]
fn std_io_error_converts_to_perfgate_error() {
    let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: PerfgateError = std_err.into();
    assert!(matches!(err, PerfgateError::Io(IoError::Other(_))));
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
fn error_exit_codes() {
    assert_eq!(ValidationError::Empty.into_perfgate_error().exit_code(), 1);
    assert_eq!(StatsError::NoSamples.into_perfgate_error().exit_code(), 1);
    assert_eq!(AdapterError::Timeout.into_perfgate_error().exit_code(), 1);
    assert_eq!(
        ConfigValidationError::BenchName("test".to_string())
            .into_perfgate_error()
            .exit_code(),
        1
    );
    assert_eq!(
        IoError::Other("test".to_string())
            .into_perfgate_error()
            .exit_code(),
        1
    );
    assert_eq!(PairedError::NoSamples.into_perfgate_error().exit_code(), 1);
}

#[test]
fn error_recoverable_timeout() {
    let err = PerfgateError::Adapter(AdapterError::Timeout);
    assert!(err.is_recoverable());
}

#[test]
fn error_not_recoverable_validation() {
    let err = PerfgateError::Validation(ValidationError::Empty);
    assert!(!err.is_recoverable());
}

#[test]
fn error_not_recoverable_empty_argv() {
    let err = PerfgateError::Adapter(AdapterError::EmptyArgv);
    assert!(!err.is_recoverable());
}

#[test]
fn error_not_recoverable_no_samples() {
    let err = PerfgateError::Stats(StatsError::NoSamples);
    assert!(!err.is_recoverable());
}

#[test]
fn validation_error_display_messages() {
    assert!(ValidationError::Empty.to_string().contains("empty"));
    assert!(
        ValidationError::TooLong {
            name: "test".to_string(),
            max_len: 64
        }
        .to_string()
        .contains("exceeds maximum length")
    );
    assert!(
        ValidationError::InvalidCharacters {
            name: "TEST".to_string()
        }
        .to_string()
        .contains("invalid characters")
    );
    assert!(
        ValidationError::EmptySegment {
            name: "/test".to_string()
        }
        .to_string()
        .contains("empty path segment")
    );
    assert!(
        ValidationError::PathTraversal {
            name: "../test".to_string(),
            segment: "..".to_string()
        }
        .to_string()
        .contains("path traversal")
    );
}

#[test]
fn adapter_error_display_messages() {
    assert!(AdapterError::EmptyArgv.to_string().contains("argv"));
    assert!(AdapterError::Timeout.to_string().contains("timed out"));
    assert!(
        AdapterError::TimeoutUnsupported
            .to_string()
            .contains("not supported")
    );
    assert!(
        AdapterError::Other("custom error".to_string())
            .to_string()
            .contains("custom error")
    );
}

#[test]
fn config_validation_error_display_messages() {
    assert!(
        ConfigValidationError::BenchName("invalid".to_string())
            .to_string()
            .contains("bench name")
    );
    assert!(
        ConfigValidationError::ConfigFile("missing field".to_string())
            .to_string()
            .contains("config")
    );
}

#[test]
fn io_error_display_messages() {
    assert!(
        IoError::BaselineResolve("not found".to_string())
            .to_string()
            .contains("baseline resolve")
    );
    assert!(
        IoError::ArtifactWrite("permission denied".to_string())
            .to_string()
            .contains("write artifacts")
    );
    assert!(
        IoError::RunCommand {
            command: "echo".to_string(),
            reason: "spawn failed".to_string(),
        }
            .to_string()
            .contains("failed to execute command")
    );
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
fn perfgate_error_from_std_io_error() {
    let std_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let err: PerfgateError = std_err.into();

    assert!(matches!(err, PerfgateError::Io(IoError::Other(_))));
}

#[test]
fn io_error_from_std_io_error() {
    let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: IoError = std_err.into();

    assert!(matches!(err, IoError::Other(_)));
}

trait IntoPerfgateError {
    fn into_perfgate_error(self) -> PerfgateError;
}

impl<T: Into<PerfgateError>> IntoPerfgateError for T {
    fn into_perfgate_error(self) -> PerfgateError {
        self.into()
    }
}

#[test]
fn error_chain_through_domain() {
    use perfgate_domain::DomainError;

    let domain_err = DomainError::NoSamples;
    let msg = domain_err.to_string();
    assert!(msg.contains("no samples"));
}

#[test]
fn error_chain_through_budget() {
    use perfgate_budget::BudgetError;

    let budget_err = BudgetError::InvalidBaseline;
    let msg = budget_err.to_string();
    assert!(msg.contains("baseline"));
}

#[test]
fn error_chain_through_stats() {
    let stats_err = StatsError::NoSamples;
    let msg = stats_err.to_string();
    assert!(msg.contains("no samples"));
}

#[test]
fn error_chain_through_paired() {
    let paired_err = PairedError::NoSamples;
    let msg = paired_err.to_string();
    assert!(msg.contains("no samples"));
}

#[test]
fn result_type_alias_works() {
    fn might_fail() -> perfgate_error::Result<String> {
        Err(PerfgateError::Validation(ValidationError::Empty))
    }

    let result = might_fail();
    assert!(result.is_err());
}
