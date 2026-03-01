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

    // ── Boundary value tests ──────────────────────────────────────────

    #[test]
    fn boundary_exact_max_len() {
        let name = "a".repeat(BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name).is_ok());
    }

    #[test]
    fn boundary_one_over_max_len() {
        let name = "a".repeat(BENCH_NAME_MAX_LEN + 1);
        assert!(matches!(
            validate_bench_name(&name),
            Err(ValidationError::TooLong { max_len, .. }) if max_len == BENCH_NAME_MAX_LEN
        ));
    }

    #[test]
    fn boundary_one_under_max_len() {
        let name = "a".repeat(BENCH_NAME_MAX_LEN - 1);
        assert!(validate_bench_name(&name).is_ok());
    }

    #[test]
    fn boundary_single_char_all_valid() {
        for c in b'a'..=b'z' {
            assert!(validate_bench_name(&String::from(c as char)).is_ok());
        }
        for c in b'0'..=b'9' {
            assert!(validate_bench_name(&String::from(c as char)).is_ok());
        }
        assert!(validate_bench_name("_").is_ok());
        assert!(validate_bench_name("-").is_ok());
    }

    #[test]
    fn boundary_single_dot_is_path_traversal() {
        assert!(matches!(
            validate_bench_name("."),
            Err(ValidationError::PathTraversal { .. })
        ));
    }

    #[test]
    fn boundary_double_dot_is_path_traversal() {
        assert!(matches!(
            validate_bench_name(".."),
            Err(ValidationError::PathTraversal { .. })
        ));
    }

    #[test]
    fn boundary_single_slash_is_empty_segment() {
        assert!(matches!(
            validate_bench_name("/"),
            Err(ValidationError::EmptySegment { .. })
        ));
    }

    #[test]
    fn boundary_max_len_with_slashes() {
        // Build a name of exactly BENCH_NAME_MAX_LEN using segments
        let segment = "ab";
        let sep = "/";
        let seg_with_sep = segment.len() + sep.len(); // 3
        let count = BENCH_NAME_MAX_LEN / seg_with_sep; // 21 segments with slashes
        let remainder = BENCH_NAME_MAX_LEN - (count * seg_with_sep);
        let mut name: String = (0..count).map(|_| format!("{segment}/")).collect();
        name.push_str(&"a".repeat(remainder));
        assert_eq!(name.len(), BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name).is_ok());
    }

    // ── Unicode handling tests ────────────────────────────────────────

    #[test]
    fn unicode_emoji_rejected() {
        assert!(matches!(
            validate_bench_name("bench-🚀"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("🔥"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("a😀b"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn unicode_cjk_rejected() {
        assert!(matches!(
            validate_bench_name("ベンチ"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench-测试"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("벤치마크"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn unicode_rtl_rejected() {
        assert!(matches!(
            validate_bench_name("مقعد"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench-בדיקה"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn unicode_accented_rejected() {
        assert!(matches!(
            validate_bench_name("café"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("naïve"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("über"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn unicode_zero_width_and_bom_rejected() {
        // Zero-width space U+200B
        assert!(matches!(
            validate_bench_name("bench\u{200B}name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        // BOM U+FEFF
        assert!(matches!(
            validate_bench_name("\u{FEFF}bench"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn unicode_multibyte_length_check() {
        // 16 x 4-byte emoji = 64 bytes, but invalid chars rejected first
        let name: String = "🔥".repeat(16);
        assert_eq!(name.len(), 64);
        assert!(matches!(
            validate_bench_name(&name),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    // ── Empty input tests ─────────────────────────────────────────────

    #[test]
    fn empty_string_returns_empty_error() {
        assert!(matches!(
            validate_bench_name(""),
            Err(ValidationError::Empty)
        ));
    }

    #[test]
    fn whitespace_only_rejected() {
        assert!(matches!(
            validate_bench_name(" "),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("   "),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("\t"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("\n"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("\r\n"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn null_byte_rejected() {
        assert!(matches!(
            validate_bench_name("\0"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("bench\0name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    // ── Negative / adversarial input tests ────────────────────────────

    #[test]
    fn hyphen_prefixed_names_are_valid() {
        // Leading hyphen is allowed since '-' is a valid character
        assert!(validate_bench_name("-1").is_ok());
        assert!(validate_bench_name("-bench").is_ok());
        assert!(validate_bench_name("--double").is_ok());
    }

    #[test]
    fn control_characters_rejected() {
        for c in 0x00u8..=0x1F {
            let name = format!("bench{}name", c as char);
            assert!(
                validate_bench_name(&name).is_err(),
                "control char 0x{c:02x} should be rejected"
            );
        }
        // DEL character (0x7F)
        assert!(matches!(
            validate_bench_name("bench\x7Fname"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn backslash_rejected() {
        assert!(matches!(
            validate_bench_name("bench\\name"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
        assert!(matches!(
            validate_bench_name("path\\to\\bench"),
            Err(ValidationError::InvalidCharacters { .. })
        ));
    }

    #[test]
    fn path_traversal_in_middle_segment() {
        assert!(matches!(
            validate_bench_name("a/../b"),
            Err(ValidationError::PathTraversal { .. })
        ));
        assert!(matches!(
            validate_bench_name("a/./b"),
            Err(ValidationError::PathTraversal { .. })
        ));
        assert!(matches!(
            validate_bench_name("a/b/../c"),
            Err(ValidationError::PathTraversal { .. })
        ));
    }

    #[test]
    fn triple_dot_segment_is_valid() {
        // "..." is not "." or ".." so it should be valid
        assert!(validate_bench_name("...").is_ok());
        assert!(validate_bench_name("a/.../b").is_ok());
    }

    // ── Large input tests ─────────────────────────────────────────────

    #[test]
    fn large_string_over_max_len() {
        let name = "a".repeat(1000);
        assert!(matches!(
            validate_bench_name(&name),
            Err(ValidationError::TooLong { .. })
        ));
    }

    #[test]
    fn large_string_way_over_max_len() {
        let name = "b".repeat(100_000);
        let result = validate_bench_name(&name);
        assert!(
            matches!(result, Err(ValidationError::TooLong { max_len, .. }) if max_len == BENCH_NAME_MAX_LEN)
        );
    }

    #[test]
    fn large_string_with_invalid_chars_over_max_len() {
        // TooLong is checked before InvalidCharacters
        let name = "X".repeat(BENCH_NAME_MAX_LEN + 1);
        assert!(matches!(
            validate_bench_name(&name),
            Err(ValidationError::TooLong { .. })
        ));
    }

    #[test]
    fn large_number_of_segments() {
        // Many valid segments within max length: "a/a/a/..."
        let segments: Vec<&str> = (0..32).map(|_| "a").collect();
        let name = segments.join("/");
        if name.len() <= BENCH_NAME_MAX_LEN {
            assert!(validate_bench_name(&name).is_ok());
        } else {
            assert!(matches!(
                validate_bench_name(&name),
                Err(ValidationError::TooLong { .. })
            ));
        }
    }

    #[test]
    fn large_segment_at_boundary() {
        // Single segment of exactly max length
        let name = "z".repeat(BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name).is_ok());
    }

    #[test]
    fn error_preserves_name_for_large_input() {
        let name = "x".repeat(BENCH_NAME_MAX_LEN + 10);
        if let Err(ValidationError::TooLong {
            name: err_name,
            max_len,
        }) = validate_bench_name(&name)
        {
            assert_eq!(err_name, name);
            assert_eq!(max_len, BENCH_NAME_MAX_LEN);
        } else {
            panic!("expected TooLong error");
        }
    }

    #[test]
    fn bench_name_max_len_constant_is_64() {
        assert_eq!(BENCH_NAME_MAX_LEN, 64);
    }

    #[test]
    fn bench_name_pattern_matches_expected() {
        assert_eq!(BENCH_NAME_PATTERN, r"^[a-z0-9_.\-/]+$");
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
            let name: String = std::iter::repeat_n(c, 10).collect();
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
