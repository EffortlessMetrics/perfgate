//! Integration tests: validation crate → types crate.
//!
//! These tests verify that perfgate-validation integrates correctly
//! with perfgate-types, including config validation with bench names.

use perfgate_types::{
    BaselineServerConfig, BenchConfigFile, ConfigFile, DefaultsConfig, validate_bench_name,
};
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
        baseline_server: BaselineServerConfig::default(),
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

            scaling: None,
        }],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn config_file_rejects_invalid_bench_names() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        baseline_server: BaselineServerConfig::default(),
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

            scaling: None,
        }],
    };

    assert!(config.validate().is_err());
}

#[test]
fn multiple_benches_all_validated() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        baseline_server: BaselineServerConfig::default(),
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

                scaling: None,
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

                scaling: None,
            },
        ],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn validation_fails_on_first_invalid_bench() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        baseline_server: BaselineServerConfig::default(),
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

                scaling: None,
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

                scaling: None,
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
        baseline_server: BaselineServerConfig::default(),
        benches: vec![],
    };

    assert!(config.validate().is_ok());
}

// --- Config validation edge cases ---

#[test]
fn config_duplicate_bench_names_passes_validation() {
    // ConfigFile::validate() only checks bench names individually,
    // so duplicates are accepted at the type level.
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        baseline_server: BaselineServerConfig::default(),
        benches: vec![
            BenchConfigFile {
                name: "same-name".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,

                scaling: None,
            },
            BenchConfigFile {
                name: "same-name".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,

                scaling: None,
            },
        ],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn config_negative_threshold_deserializes() {
    let toml_str = r#"
[defaults]
threshold = -0.5
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.defaults.threshold, Some(-0.5));
    assert!(config.validate().is_ok());
}

#[test]
fn config_threshold_greater_than_one_deserializes() {
    let toml_str = r#"
[defaults]
threshold = 1.5
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.defaults.threshold, Some(1.5));
    assert!(config.validate().is_ok());
}

#[test]
fn config_nan_threshold_in_toml_deserializes_as_nan() {
    let toml_str = r#"
[defaults]
threshold = nan
"#;
    // TOML spec allows nan/inf as float values
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.defaults.threshold.unwrap().is_nan());
}

#[test]
fn config_infinity_threshold_in_toml_deserializes_as_inf() {
    let toml_str = r#"
[defaults]
threshold = inf
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.defaults.threshold.unwrap().is_infinite());
}

#[test]
fn config_budget_override_negative_threshold() {
    let toml_str = r#"
[[bench]]
name = "my-bench"
command = ["echo"]

[bench.budgets.wall_ms]
threshold = -0.1
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    let budget = config.benches[0].budgets.as_ref().unwrap();
    assert_eq!(
        budget
            .get(&perfgate_types::Metric::WallMs)
            .unwrap()
            .threshold,
        Some(-0.1)
    );
}

#[test]
fn config_budget_override_threshold_greater_than_one() {
    let toml_str = r#"
[[bench]]
name = "my-bench"
command = ["echo"]

[bench.budgets.wall_ms]
threshold = 2.0
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    let budget = config.benches[0].budgets.as_ref().unwrap();
    assert_eq!(
        budget
            .get(&perfgate_types::Metric::WallMs)
            .unwrap()
            .threshold,
        Some(2.0)
    );
}

#[test]
fn config_invalid_metric_name_in_toml_is_rejected() {
    let toml_str = r#"
[[bench]]
name = "my-bench"
command = ["echo"]
metrics = ["not_a_real_metric"]
"#;
    assert!(toml::from_str::<ConfigFile>(toml_str).is_err());
}

#[test]
fn config_valid_metric_names_in_toml() {
    let toml_str = r#"
[[bench]]
name = "my-bench"
command = ["echo"]
metrics = ["wall_ms", "max_rss_kb", "cpu_ms"]
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    let metrics = config.benches[0].metrics.as_ref().unwrap();
    assert_eq!(metrics.len(), 3);
}

#[test]
fn config_empty_bench_name_rejected() {
    let config = ConfigFile {
        defaults: DefaultsConfig::default(),
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: String::new(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,

            scaling: None,
        }],
    };

    let err = config.validate().unwrap_err();
    assert!(err.contains("empty"));
}

#[test]
fn config_path_traversal_variants_rejected() {
    for name in &["../evil", "bench/../etc/passwd", "./sneaky", "a/../../b"] {
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            benches: vec![BenchConfigFile {
                name: name.to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,

                scaling: None,
            }],
        };

        assert!(
            config.validate().is_err(),
            "expected validation to reject path traversal: {name}"
        );
    }
}

#[test]
fn config_zero_threshold_deserializes() {
    let toml_str = r#"
[defaults]
threshold = 0.0
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.defaults.threshold, Some(0.0));
}

#[test]
fn config_warn_factor_edge_values() {
    let toml_str = r#"
[defaults]
warn_factor = 0.0
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.defaults.warn_factor, Some(0.0));

    let toml_str = r#"
[defaults]
warn_factor = -1.0
"#;
    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert_eq!(config.defaults.warn_factor, Some(-1.0));
}
