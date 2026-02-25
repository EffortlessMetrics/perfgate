//! Basic example demonstrating benchmark name validation.
//!
//! Run with: cargo run -p perfgate-validation --example basic

use perfgate_validation::{BENCH_NAME_MAX_LEN, ValidationError, validate_bench_name};

fn main() {
    println!("=== perfgate-validation Basic Example ===\n");

    println!("1. Valid benchmark names:");
    let valid_names = [
        "my-bench",
        "bench_v2",
        "path/to/bench",
        "bench.v1",
        "a1-b2_c3.d4/e5",
        "category/subcategory/benchmark",
    ];

    for name in valid_names {
        match validate_bench_name(name) {
            Ok(()) => println!("   VALID: \"{}\"", name),
            Err(e) => println!("   INVALID: \"{}\" - {}", name, e),
        }
    }

    println!("\n2. Invalid names - empty:");
    match validate_bench_name("") {
        Err(ValidationError::Empty) => println!("   Correctly rejected empty string"),
        _ => println!("   Unexpected result for empty string"),
    }

    println!("\n3. Invalid names - uppercase letters:");
    let uppercase_names = ["MyBench", "BENCH", "benchA"];
    for name in uppercase_names {
        match validate_bench_name(name) {
            Err(ValidationError::InvalidCharacters { .. }) => {
                println!("   Correctly rejected \"{}\" (uppercase not allowed)", name)
            }
            _ => println!("   Unexpected result for \"{}\"", name),
        }
    }

    println!("\n4. Invalid names - special characters:");
    let special_char_names = ["bench name", "bench@name", "bench|name", "bench#name"];
    for name in special_char_names {
        match validate_bench_name(name) {
            Err(ValidationError::InvalidCharacters { name: n }) => {
                println!("   Correctly rejected \"{}\" (invalid chars)", n)
            }
            _ => println!("   Unexpected result for \"{}\"", name),
        }
    }

    println!("\n5. Invalid names - path traversal:");
    let traversal_names = ["../bench", "bench/../x", "./bench", "bench/."];
    for name in traversal_names {
        match validate_bench_name(name) {
            Err(ValidationError::PathTraversal { name: n, segment }) => {
                println!("   Correctly rejected \"{}\" (segment: \"{}\")", n, segment)
            }
            _ => println!("   Unexpected result for \"{}\"", name),
        }
    }

    println!("\n6. Invalid names - empty segments:");
    let empty_segment_names = ["/bench", "bench/", "bench//x", "a//b"];
    for name in empty_segment_names {
        match validate_bench_name(name) {
            Err(ValidationError::EmptySegment { name: n }) => {
                println!("   Correctly rejected \"{}\" (empty segment)", n)
            }
            _ => println!("   Unexpected result for \"{}\"", name),
        }
    }

    println!("\n7. Name length limits:");
    let name_64: String = "a".repeat(BENCH_NAME_MAX_LEN);
    match validate_bench_name(&name_64) {
        Ok(()) => println!("   Name with {} chars: VALID", BENCH_NAME_MAX_LEN),
        Err(e) => println!("   Unexpected error: {}", e),
    }

    let name_65: String = "a".repeat(BENCH_NAME_MAX_LEN + 1);
    match validate_bench_name(&name_65) {
        Err(ValidationError::TooLong { max_len, .. }) => {
            println!(
                "   Name with {} chars: REJECTED (max: {})",
                name_65.len(),
                max_len
            )
        }
        _ => println!("   Unexpected result for too-long name"),
    }

    println!("\n8. Getting error name accessor:");
    if let Err(err) = validate_bench_name("INVALID") {
        println!("   Error name accessor: \"{}\"", err.name());
    }

    println!("\n=== Example complete ===");
}
