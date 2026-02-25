//! Validation functions for benchmark names and configuration.
//!
//! This crate provides validation logic for validating benchmark names
//! according to a strict set of rules.
//!
//! # Example
//!
//! ```
//! use perfgate_validation::{validate_bench_name, ValidationError};
//!
//! // Valid names
//! assert!(validate_bench_name("my-bench").is_ok());
//! assert!(validate_bench_name("bench_v2").is_ok());
//! assert!(validate_bench_name("path/to/bench").is_ok());
//! assert!(validate_bench_name("bench.v1").is_ok());
//!
//! // Invalid names
//! assert!(validate_bench_name("").is_err());
//! assert!(validate_bench_name("MyBench").is_err());  // uppercase
//! assert!(validate_bench_name("../bench").is_err()); // path traversal
//! assert!(validate_bench_name("bench/").is_err());   // trailing slash
//! ```

pub use perfgate_error::{
    BENCH_NAME_MAX_LEN, BENCH_NAME_PATTERN, ValidationError, validate_bench_name,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_names_basic() {
        assert!(validate_bench_name("my-bench").is_ok());
        assert!(validate_bench_name("bench_a").is_ok());
        assert!(validate_bench_name("path/to/bench").is_ok());
        assert!(validate_bench_name("bench.v2").is_ok());
        assert!(validate_bench_name("a").is_ok());
        assert!(validate_bench_name("123").is_ok());
    }

    #[test]
    fn valid_names_with_dots() {
        assert!(validate_bench_name("bench.v1").is_ok());
        assert!(validate_bench_name("v1.2.3").is_ok());
        assert!(validate_bench_name("bench.test.final").is_ok());
    }

    #[test]
    fn valid_names_with_hyphens() {
        assert!(validate_bench_name("my-bench-name").is_ok());
        assert!(validate_bench_name("bench-v1-final").is_ok());
    }

    #[test]
    fn valid_names_with_underscores() {
        assert!(validate_bench_name("bench_name").is_ok());
        assert!(validate_bench_name("my_bench_v2").is_ok());
    }

    #[test]
    fn valid_names_with_slashes() {
        assert!(validate_bench_name("path/to/bench").is_ok());
        assert!(validate_bench_name("a/b/c").is_ok());
        assert!(validate_bench_name("category/subcategory/bench").is_ok());
    }

    #[test]
    fn valid_names_mixed_chars() {
        assert!(validate_bench_name("my_bench-v1.2").is_ok());
        assert!(validate_bench_name("path/to-bench_v2").is_ok());
        assert!(validate_bench_name("a1-b2_c3.d4/e5").is_ok());
    }

    #[test]
    fn valid_names_single_char() {
        assert!(validate_bench_name("a").is_ok());
        assert!(validate_bench_name("z").is_ok());
        assert!(validate_bench_name("0").is_ok());
        assert!(validate_bench_name("9").is_ok());
    }

    #[test]
    fn valid_names_all_digits() {
        assert!(validate_bench_name("12345").is_ok());
        assert!(validate_bench_name("0").is_ok());
    }

    #[test]
    fn invalid_empty() {
        assert!(matches!(
            validate_bench_name(""),
            Err(ValidationError::Empty)
        ));
    }

    #[test]
    fn invalid_uppercase() {
        assert!(matches!(
            validate_bench_name("MyBench"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("BENCH"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("benchA"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("Bench"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn invalid_special_characters() {
        assert!(matches!(
            validate_bench_name("bench|name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench@name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench#name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench$name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench%name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench!name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn invalid_path_traversal() {
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
        assert!(matches!(
            validate_bench_name("bench/."),
            Err(ValidationError::PathTraversal { .. })
        ));
        assert!(matches!(
            validate_bench_name(".."),
            Err(ValidationError::PathTraversal { .. })
        ));
        assert!(matches!(
            validate_bench_name("."),
            Err(ValidationError::PathTraversal { .. })
        ));
    }

    #[test]
    fn invalid_empty_segments() {
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
        assert!(matches!(
            validate_bench_name("/"),
            Err(ValidationError::EmptySegment { .. })
        ));
        assert!(matches!(
            validate_bench_name("a//b"),
            Err(ValidationError::EmptySegment { .. })
        ));
        assert!(matches!(
            validate_bench_name("//"),
            Err(ValidationError::EmptySegment { .. })
        ));
    }

    #[test]
    fn invalid_too_long() {
        let name_64 = "a".repeat(BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name_64).is_ok());

        let name_65 = "a".repeat(BENCH_NAME_MAX_LEN + 1);
        let result = validate_bench_name(&name_65);
        assert!(matches!(result, Err(ValidationError::TooLong { .. })));
        if let Err(ValidationError::TooLong { max_len, .. }) = result {
            assert_eq!(max_len, BENCH_NAME_MAX_LEN);
        }
    }

    #[test]
    fn error_name_accessor() {
        let err = validate_bench_name("INVALID").unwrap_err();
        assert_eq!(err.name(), "INVALID");

        let err = validate_bench_name("").unwrap_err();
        assert_eq!(err.name(), "");

        let err = validate_bench_name(&"x".repeat(100)).unwrap_err();
        assert!(err.name().starts_with('x'));
    }

    #[test]
    fn error_display() {
        let err = ValidationError::Empty;
        assert!(err.to_string().contains("must not be empty"));

        let err = ValidationError::TooLong {
            name: "test".to_string(),
            max_len: 64,
        };
        assert!(err.to_string().contains("exceeds maximum length"));

        let err = ValidationError::InvalidCharacters {
            name: "TEST".to_string(),
        };
        assert!(err.to_string().contains("invalid characters"));

        let err = ValidationError::EmptySegment {
            name: "/test".to_string(),
        };
        assert!(err.to_string().contains("empty path segment"));

        let err = ValidationError::PathTraversal {
            name: "../test".to_string(),
            segment: "..".to_string(),
        };
        assert!(err.to_string().contains("path traversal"));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        fn valid_bench_char()(
            c in any::<u8>()
                .prop_map(|b| {
                    if b.is_ascii_lowercase() || b.is_ascii_digit() {
                        char::from(b)
                    } else {
                        ['_', '-'][(b as usize) % 2]
                    }
                })
        ) -> char {
            c
        }
    }

    prop_compose! {
        fn valid_segment_char()(
            c in any::<u8>()
                .prop_map(|b| {
                    if b.is_ascii_lowercase() || b.is_ascii_digit() {
                        char::from(b)
                    } else {
                        ['_', '.', '-'][(b as usize) % 3]
                    }
                })
        ) -> char {
            c
        }
    }

    prop_compose! {
        fn valid_segment()(s in proptest::collection::vec(valid_segment_char(), 1..10)) -> String {
            let seg: String = s.into_iter().collect();
            if seg == "." || seg == ".." {
                "a".to_string()
            } else {
                seg
            }
        }
    }

    prop_compose! {
        fn valid_bench_name()(
            segments in proptest::collection::vec(valid_segment(), 1..5)
        ) -> String {
            segments.join("/")
        }
    }

    fn is_invalid_chars_error(result: &std::result::Result<(), ValidationError>) -> bool {
        matches!(result, Err(ValidationError::InvalidCharacters { .. }))
    }

    fn is_too_long_error(result: &std::result::Result<(), ValidationError>) -> bool {
        matches!(result, Err(ValidationError::TooLong { .. }))
    }

    fn is_empty_error(result: &std::result::Result<(), ValidationError>) -> bool {
        matches!(result, Err(ValidationError::Empty))
    }

    fn is_empty_segment_error(result: &std::result::Result<(), ValidationError>) -> bool {
        matches!(result, Err(ValidationError::EmptySegment { .. }))
    }

    fn is_path_traversal_error(result: &std::result::Result<(), ValidationError>) -> bool {
        matches!(result, Err(ValidationError::PathTraversal { .. }))
    }

    proptest! {
        #[test]
        fn valid_chars_produce_ok(name in valid_bench_name()) {
            prop_assume!(name.len() <= BENCH_NAME_MAX_LEN);
            prop_assert!(validate_bench_name(&name).is_ok());
        }

        #[test]
        fn uppercase_always_fails(name in "[a-z0-9_\\-]{1,30}[A-Z][a-z0-9_\\-]{1,30}") {
            prop_assume!(name.len() <= BENCH_NAME_MAX_LEN);
            let result = validate_bench_name(&name);
            prop_assert!(is_invalid_chars_error(&result),
                "Expected InvalidCharacters error for name '{}' with uppercase, got {:?}", name, result);
        }

        #[test]
        fn length_boundary(
            len in BENCH_NAME_MAX_LEN.saturating_sub(1)..=BENCH_NAME_MAX_LEN.saturating_add(1)
        ) {
            let name: String = "a".repeat(len);
            let result = validate_bench_name(&name);
            if len <= BENCH_NAME_MAX_LEN && len > 0 {
                prop_assert!(result.is_ok());
            } else if len > BENCH_NAME_MAX_LEN {
                prop_assert!(is_too_long_error(&result));
            } else {
                prop_assert!(is_empty_error(&result));
            }
        }

        #[test]
        fn empty_string_fails(name in "") {
            let _ = name;
            let result = validate_bench_name("");
            prop_assert!(is_empty_error(&result));
        }

        #[test]
        fn double_slash_fails(prefix in valid_segment(), suffix in valid_segment()) {
            prop_assume!(prefix != "." && prefix != "..");
            prop_assume!(suffix != "." && suffix != "..");
            let name = format!("{prefix}//{suffix}");
            let result = validate_bench_name(&name);
            prop_assert!(is_empty_segment_error(&result));
        }

        #[test]
        fn leading_slash_fails(name in valid_bench_name()) {
            let name_with_leading = format!("/{name}");
            let result = validate_bench_name(&name_with_leading);
            prop_assert!(is_empty_segment_error(&result));
        }

        #[test]
        fn trailing_slash_fails(name in valid_bench_name()) {
            let name_with_trailing = format!("{name}/");
            let result = validate_bench_name(&name_with_trailing);
            prop_assert!(is_empty_segment_error(&result));
        }

        #[test]
        fn dot_segment_fails(suffix in "[a-z0-9_-]+") {
            let name = format!("./{suffix}");
            prop_assume!(!suffix.is_empty());
            let result = validate_bench_name(&name);
            prop_assert!(is_path_traversal_error(&result));
        }

        #[test]
        fn double_dot_segment_fails(suffix in "[a-z0-9_-]+") {
            let name = format!("../{suffix}");
            prop_assume!(!suffix.is_empty());
            let result = validate_bench_name(&name);
            prop_assert!(is_path_traversal_error(&result));
        }

        #[test]
        fn valid_char_roundtrip(c in valid_bench_char()) {
            let name: String = std::iter::repeat(c).take(10).collect();
            prop_assume!(name.len() <= BENCH_NAME_MAX_LEN);
            prop_assert!(validate_bench_name(&name).is_ok());
        }

        #[test]
        fn special_invalid_chars(c in any::<char>()) {
            prop_assume!(!c.is_ascii_lowercase());
            prop_assume!(!c.is_ascii_digit());
            prop_assume!(c != '_');
            prop_assume!(c != '.');
            prop_assume!(c != '-');
            prop_assume!(c != '/');
            prop_assume!(c != '\0');

            let name = format!("bench{}test", c);
            let result = validate_bench_name(&name);
            prop_assert!(is_invalid_chars_error(&result));
        }
    }
}
