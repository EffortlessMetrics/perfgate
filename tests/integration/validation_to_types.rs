//! Integration tests: validation crate → types crate.
//!
//! These tests verify that perfgate-validation integrates correctly
//! with perfgate-types, including config validation with bench names.

use perfgate_types::{BenchConfigFile, ConfigFile, DefaultsConfig, validate_bench_name};
use perfgate_validation::ValidationError;

#[test]
fn validation_error_is_used_by_types() {
    let result = validate_bench_name("");
    assert!(matches!(result, Err(ValidationError::Empty)));
}

#[test]
fn valid_bench_names_pass_validation() {
    assert!(validate_bench_name("my-bench").is_ok());
    assert!(validate_bench_name("bench_v2").is_ok());
    assert!(validate_bench_name("path/to/bench").is_ok());
    assert!(validate_bench_name("bench.v1").is_ok());
}

#[test]
fn invalid_bench_names_fail_validation() {
    assert!(validate_bench_name("").is_err());
    assert!(validate_bench_name("MyBench").is_err());
    assert!(validate_bench_name("../bench").is_err());
    assert!(validate_bench_name("bench/").is_err());
    assert!(validate_bench_name("bench//x").is_err());
}

#[test]
fn config_file_validates_bench_names() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        benches: vec![BenchConfigFile {
            name: "valid-bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn config_file_rejects_invalid_bench_names() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        benches: vec![BenchConfigFile {
            name: "../evil".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    assert!(config.validate().is_err());
}

#[test]
fn multiple_benches_all_validated() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        benches: vec![
            BenchConfigFile {
                name: "valid-bench".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,
            },
            BenchConfigFile {
                name: "also-valid".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,
            },
        ],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn validation_fails_on_first_invalid_bench() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        benches: vec![
            BenchConfigFile {
                name: "valid-bench".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,
            },
            BenchConfigFile {
                name: "Invalid".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,
            },
        ],
    };

    assert!(config.validate().is_err());
}

#[test]
fn path_traversal_is_detected() {
    assert!(matches!(
        validate_bench_name("../bench"),
        Err(ValidationError::PathTraversal { .. })
    ));
    assert!(matches!(
        validate_bench_name("bench/../x"),
        Err(ValidationError::PathTraversal { .. })
    ));
    assert!(matches!(
        validate_bench_name("./bench"),
        Err(ValidationError::PathTraversal { .. })
    ));
}

#[test]
fn empty_segments_are_detected() {
    assert!(matches!(
        validate_bench_name("/bench"),
        Err(ValidationError::EmptySegment { .. })
    ));
    assert!(matches!(
        validate_bench_name("bench/"),
        Err(ValidationError::EmptySegment { .. })
    ));
    assert!(matches!(
        validate_bench_name("bench//x"),
        Err(ValidationError::EmptySegment { .. })
    ));
}

#[test]
fn uppercase_characters_are_rejected() {
    assert!(matches!(
        validate_bench_name("MyBench"),
        Err(ValidationError::InvalidCharacters { .. })
    ));
    assert!(matches!(
        validate_bench_name("BENCH"),
        Err(ValidationError::InvalidCharacters { .. })
    ));
}

#[test]
fn too_long_names_are_rejected() {
    use perfgate_validation::BENCH_NAME_MAX_LEN;

    let long_name = "a".repeat(BENCH_NAME_MAX_LEN + 1);
    assert!(matches!(
        validate_bench_name(&long_name),
        Err(ValidationError::TooLong { .. })
    ));

    let max_name = "a".repeat(BENCH_NAME_MAX_LEN);
    assert!(validate_bench_name(&max_name).is_ok());
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
fn validation_error_display() {
    let err = ValidationError::Empty;
    assert!(err.to_string().contains("empty"));

    let err = ValidationError::PathTraversal {
        name: "../test".to_string(),
        segment: "..".to_string(),
    };
    assert!(err.to_string().contains("path traversal"));
}

#[test]
fn config_empty_benches_is_valid() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        benches: vec![],
    };

    assert!(config.validate().is_ok());
}
