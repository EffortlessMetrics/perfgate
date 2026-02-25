//! Basic example demonstrating SHA-256 hash computation.
//!
//! Run with: cargo run -p perfgate-sha256 --example basic

use perfgate_sha256::sha256_hex;

fn main() {
    println!("=== perfgate-sha256 Basic Example ===\n");

    println!("1. Computing SHA-256 hashes:");
    let hash_hello = sha256_hex(b"hello");
    println!("   sha256(b\"hello\") = {}", hash_hello);

    let hash_world = sha256_hex(b"world");
    println!("   sha256(b\"world\") = {}", hash_world);

    let hash_empty = sha256_hex(b"");
    println!("   sha256(b\"\") = {}", hash_empty);

    println!("\n2. Demonstrating determinism:");
    let hash1 = sha256_hex(b"test input");
    let hash2 = sha256_hex(b"test input");
    println!("   First call:  {}", hash1);
    println!("   Second call: {}", hash2);
    println!("   Same result: {}", hash1 == hash2);

    println!("\n3. Verifying known test vectors (NIST):");
    let test_vectors = [
        (
            b"" as &[u8],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"hello",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
        (
            b"hello world",
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        ),
    ];

    for (input, expected) in test_vectors {
        let computed = sha256_hex(input);
        let input_str = if input.is_empty() {
            "(empty)"
        } else {
            core::str::from_utf8(input).unwrap()
        };
        let status = if computed == expected { "PASS" } else { "FAIL" };
        println!("   [{}] \"{}\" -> {}", status, input_str, computed);
    }

    println!("\n4. Hash properties:");
    let data = b"fingerprint data";
    let hash = sha256_hex(data);
    println!("   Output length: {} characters (256 bits)", hash.len());
    println!(
        "   All lowercase hex: {}",
        hash.chars().all(|c| c.is_ascii_hexdigit())
    );

    println!("\n5. Different inputs produce different hashes:");
    let hash_a = sha256_hex(b"a");
    let hash_b = sha256_hex(b"b");
    println!("   sha256(b\"a\") != sha256(b\"b\"): {}", hash_a != hash_b);

    println!("\n=== Example complete ===");
}
