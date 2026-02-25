//! Minimal SHA-256 implementation for perfgate fingerprinting.
//!
//! This crate provides a `#![no_std]`-compatible SHA-256 hash function
//! that returns a hexadecimal string. It's designed for fingerprinting
//! and identification purposes, not for cryptographic security.
//!
//! # Features
//!
//! - `std` (default): Enable `std` support
//! - Without `std`: Uses `alloc::string::String`
//!
//! # Example
//!
//! ```
//! use perfgate_sha256::sha256_hex;
//!
//! let hash = sha256_hex(b"hello");
//! assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
//!
//! let empty_hash = sha256_hex(b"");
//! assert_eq!(empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
//! ```

#![no_std]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::string::String;

/// SHA-256 round constants.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Initial hash values (first 32 bits of the fractional parts of the
/// square roots of the first 8 primes 2..19).
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Compute SHA-256 hash and return as lowercase hexadecimal string.
///
/// # Arguments
///
/// * `data` - Input bytes to hash
///
/// # Returns
///
/// A 64-character lowercase hexadecimal string representing the SHA-256 hash.
///
/// # Example
///
/// ```
/// use perfgate_sha256::sha256_hex;
///
/// let hash = sha256_hex(b"hello world");
/// assert_eq!(hash.len(), 64);
/// assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
/// ```
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = H0;

    let ml = (data.len() as u64) * 8;
    let mut padded = alloc::vec::Vec::with_capacity(data.len() + 65);
    padded.extend_from_slice(data);
    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }

    padded.extend_from_slice(&ml.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];

        for (i, word_bytes) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        }

        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = String::with_capacity(64);
    for val in h.iter() {
        let _ = core::fmt::write(&mut result, core::format_args!("{:08x}", val));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    fn is_valid_hex(s: &str) -> bool {
        s.len() == 64 && s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    }

    #[test]
    fn nist_empty_string() {
        let hash = sha256_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn nist_abc() {
        let hash = sha256_hex(b"abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn nist_hello() {
        let hash = sha256_hex(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn nist_hello_world() {
        let hash = sha256_hex(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn nist_abc_long() {
        let input = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let hash = sha256_hex(input);
        assert_eq!(
            hash,
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn nist_one_million_a() {
        let input: Vec<u8> = vec![b'a'; 1_000_000];
        let hash = sha256_hex(&input);
        assert_eq!(
            hash,
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn single_byte_zero() {
        let hash = sha256_hex(b"\x00");
        assert_eq!(
            hash,
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"
        );
    }

    #[test]
    fn single_byte_ff() {
        let hash = sha256_hex(b"\xff");
        assert_eq!(
            hash,
            "a8100ae6aa1940d0b663bb31cd466142ebbdbd5187131b92d93818987832eb89"
        );
    }

    #[test]
    fn large_input_1mb() {
        let input: Vec<u8> = (0..=255u8).cycle().take(1_048_576).collect();
        let hash = sha256_hex(&input);
        assert_eq!(hash.len(), 64);
        assert!(is_valid_hex(&hash));
    }

    #[test]
    fn output_length_is_64() {
        assert_eq!(sha256_hex(b"").len(), 64);
        assert_eq!(sha256_hex(b"a").len(), 64);
        assert_eq!(sha256_hex(b"abc").len(), 64);
        assert_eq!(sha256_hex(&vec![0u8; 1000]).len(), 64);
    }

    #[test]
    fn output_is_lowercase_hex() {
        let hash = sha256_hex(b"test");
        assert!(is_valid_hex(&hash), "Output should be lowercase hex");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn determinism(bytes in proptest::collection::vec(any::<u8>(), 0..1000)) {
            let hash1 = sha256_hex(&bytes);
            let hash2 = sha256_hex(&bytes);
            prop_assert_eq!(hash1, hash2, "Same input should produce same output");
        }

        #[test]
        fn length_invariant(bytes in proptest::collection::vec(any::<u8>(), 0..1000)) {
            let hash = sha256_hex(&bytes);
            prop_assert_eq!(hash.len(), 64, "Output should always be 64 hex chars");
        }

        #[test]
        fn hex_charset(bytes in proptest::collection::vec(any::<u8>(), 0..1000)) {
            let hash = sha256_hex(&bytes);
            prop_assert!(
                hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
                "Output should only contain [0-9a-f]"
            );
        }

        #[test]
        fn different_inputs_different_outputs(
            a in proptest::collection::vec(any::<u8>(), 1..100),
            b in proptest::collection::vec(any::<u8>(), 1..100)
        ) {
            prop_assume!(a != b);
            let hash_a = sha256_hex(&a);
            let hash_b = sha256_hex(&b);
            prop_assert_ne!(hash_a, hash_b, "Different inputs should produce different outputs");
        }
    }
}
