//! Comprehensive correctness tests for SIMD implementations.
//!
//! These tests ensure that SIMD implementations produce identical results
//! to their scalar counterparts across all edge cases and input patterns.

use checkle::simd::{bytes_to_hex, bytes_to_hex_scalar, is_hex_string};
use proptest::prelude::*;
use rstest::rstest;

/// Test that empty input produces empty output for both implementations
#[test]
fn test_empty_input() {
    let input = vec![];
    let scalar_result = bytes_to_hex_scalar(&input);
    let unified_result = bytes_to_hex(&input);

    assert_eq!(scalar_result, "");
    assert_eq!(unified_result, "");
    assert_eq!(scalar_result, unified_result);
}

/// Test single byte conversions
#[rstest]
#[case(0x00, "00")]
#[case(0x0F, "0f")]
#[case(0xF0, "f0")]
#[case(0xFF, "ff")]
#[case(0x42, "42")]
#[case(0xDE, "de")]
#[case(0xAD, "ad")]
#[case(0xBE, "be")]
#[case(0xEF, "ef")]
fn test_single_byte(#[case] byte: u8, #[case] expected: &str) {
    let input = vec![byte];
    let scalar_result = bytes_to_hex_scalar(&input);
    let unified_result = bytes_to_hex(&input);

    assert_eq!(scalar_result, expected);
    assert_eq!(unified_result, expected);
    assert_eq!(scalar_result, unified_result);
}

/// Test common hash sizes
#[rstest]
#[case(16)] // MD5
#[case(20)] // SHA-1
#[case(32)] // SHA-256
#[case(48)] // SHA-384
#[case(64)] // SHA-512
fn test_hash_sizes(#[case] size: usize) {
    let input: Vec<u8> = (0..size)
        .map(|i| u8::try_from((i * 7) % 256).expect("modulo 256 always fits in u8"))
        .collect();
    let scalar_result = bytes_to_hex_scalar(&input);
    let unified_result = bytes_to_hex(&input);

    assert_eq!(scalar_result.len(), size * 2);
    assert_eq!(unified_result.len(), size * 2);
    assert_eq!(scalar_result, unified_result);
}

/// Test SIMD boundary conditions
#[rstest]
#[case(31)] // Just under SIMD chunk size
#[case(32)] // Exact SIMD chunk size
#[case(33)] // Just over SIMD chunk size
#[case(63)] // Just under 2x SIMD chunk size
#[case(64)] // Exact 2x SIMD chunk size
#[case(65)] // Just over 2x SIMD chunk size
#[case(127)] // Multiple chunks plus remainder
#[case(128)] // Exact multiple of 32
#[case(129)] // Multiple chunks plus 1
fn test_simd_boundaries(#[case] size: usize) {
    let input: Vec<u8> = (0..size)
        .map(|i| u8::try_from(i % 256).expect("modulo 256 always fits in u8"))
        .collect();
    let scalar_result = bytes_to_hex_scalar(&input);
    let unified_result = bytes_to_hex(&input);

    assert_eq!(scalar_result.len(), size * 2);
    assert_eq!(unified_result.len(), size * 2);
    assert_eq!(scalar_result, unified_result);
}

/// Test all possible byte values
#[test]
fn test_all_byte_values() {
    for byte in 0u8..=255 {
        let input = vec![byte];
        let scalar_result = bytes_to_hex_scalar(&input);
        let unified_result = bytes_to_hex(&input);

        assert_eq!(scalar_result, unified_result);
        assert_eq!(scalar_result.len(), 2);
        assert!(is_hex_string(&scalar_result));
        assert!(scalar_result.chars().all(|c| !c.is_ascii_uppercase()));
    }
}

/// Test known test vectors
#[test]
fn test_known_vectors() {
    let test_cases = vec![
        (vec![0xDE, 0xAD, 0xBE, 0xEF], "deadbeef"),
        (vec![0x00, 0x01, 0x02, 0x03], "00010203"),
        (vec![0xFF, 0xFE, 0xFD, 0xFC], "fffefdfc"),
        (
            vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0],
            "123456789abcdef0",
        ),
    ];

    for (input, expected) in test_cases {
        let scalar_result = bytes_to_hex_scalar(&input);
        let unified_result = bytes_to_hex(&input);

        assert_eq!(scalar_result, expected);
        assert_eq!(unified_result, expected);
    }
}

/// Test large buffers
#[rstest]
#[case(256)]
#[case(512)]
#[case(1024)]
#[case(2048)]
#[case(4096)]
fn test_large_buffers(#[case] size: usize) {
    let input: Vec<u8> = (0..size)
        .map(|i| u8::try_from((i * 13) % 256).expect("modulo 256 always fits in u8"))
        .collect();
    let scalar_result = bytes_to_hex_scalar(&input);
    let unified_result = bytes_to_hex(&input);

    assert_eq!(scalar_result.len(), size * 2);
    assert_eq!(unified_result.len(), size * 2);
    assert_eq!(scalar_result, unified_result);
}

/// Test repeating patterns
#[test]
fn test_repeating_patterns() {
    // All zeros
    let zeros = vec![0x00; 128];
    assert_eq!(bytes_to_hex_scalar(&zeros), bytes_to_hex(&zeros));

    // All ones (0xFF)
    let ones = vec![0xFF; 128];
    assert_eq!(bytes_to_hex_scalar(&ones), bytes_to_hex(&ones));

    // Alternating pattern
    let alternating: Vec<u8> = (0..128)
        .map(|i| if i % 2 == 0 { 0xAA } else { 0x55 })
        .collect();
    assert_eq!(
        bytes_to_hex_scalar(&alternating),
        bytes_to_hex(&alternating)
    );
}

// Property-based testing
proptest! {
    #[test]
    fn prop_scalar_unified_equivalence(bytes in prop::collection::vec(any::<u8>(), 0..=1024)) {
        let scalar_result = bytes_to_hex_scalar(&bytes);
        let unified_result = bytes_to_hex(&bytes);
        prop_assert_eq!(scalar_result, unified_result);
    }

    #[test]
    fn prop_output_format(bytes in prop::collection::vec(any::<u8>(), 0..=1024)) {
        let result = bytes_to_hex(&bytes);

        // Length is exactly 2x input
        prop_assert_eq!(result.len(), bytes.len() * 2);

        // All characters are valid lowercase hex
        prop_assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
        prop_assert!(result.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn prop_reversibility(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
        let hex_string = bytes_to_hex(&bytes);
        let decoded = hex::decode(&hex_string).expect("hex decode should always succeed for hex output");
        prop_assert_eq!(decoded, bytes);
    }
}

/// Test integration with actual hash outputs
#[test]
fn test_real_hash_integration() {
    use md5::{Digest, Md5};
    use sha2::Sha256;

    let data = b"Hello, World!";

    // MD5 hash
    let mut md5_hasher = Md5::new();
    md5_hasher.update(data);
    let md5_bytes = md5_hasher.finalize();
    let md5_scalar = bytes_to_hex_scalar(&md5_bytes);
    let md5_unified = bytes_to_hex(&md5_bytes);
    assert_eq!(md5_scalar, md5_unified);
    assert_eq!(md5_unified, "65a8e27d8879283831b664bd8b7f0ad4");

    // SHA256 hash
    let mut sha256_hasher = Sha256::new();
    sha256_hasher.update(data);
    let sha256_bytes = sha256_hasher.finalize();
    let sha256_scalar = bytes_to_hex_scalar(&sha256_bytes);
    let sha256_unified = bytes_to_hex(&sha256_bytes);
    assert_eq!(sha256_scalar, sha256_unified);
    assert_eq!(
        sha256_unified,
        "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
    );
}

/// Test thread safety
#[test]
fn test_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let test_data: Vec<Vec<u8>> = (0..100)
        .map(|i| {
            (0..32)
                .map(|j| u8::try_from((i + j) % 256).expect("modulo 256 always fits in u8"))
                .collect()
        })
        .collect();

    let test_data = Arc::new(test_data);
    let mut handles = vec![];

    for i in 0..10 {
        let data = Arc::clone(&test_data);
        let handle = thread::spawn(move || {
            for (j, bytes) in data.iter().enumerate() {
                if j % 10 == i {
                    let result = bytes_to_hex(bytes);
                    assert_eq!(result.len(), bytes.len() * 2);
                    assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should not panic");
    }
}

#[cfg(feature = "simd")]
mod simd_specific_tests {
    use super::*;
    use checkle::simd::bytes_to_hex_simd;

    /// Test SIMD implementation directly
    #[test]
    fn test_simd_direct() {
        let input = vec![0xAB; 64];
        let simd_result = bytes_to_hex_simd(&input);
        let scalar_result = bytes_to_hex_scalar(&input);

        assert_eq!(simd_result, scalar_result);
        assert_eq!(simd_result, "ab".repeat(64));
    }

    // Test SIMD with various chunk alignments
    proptest! {
        #[test]
        fn prop_simd_scalar_exact_equivalence(bytes in prop::collection::vec(any::<u8>(), 0..=512)) {
            let simd_result = bytes_to_hex_simd(&bytes);
            let scalar_result = bytes_to_hex_scalar(&bytes);

            // Verify character-by-character equivalence
            let simd_chars: Vec<char> = simd_result.chars().collect();
            let scalar_chars: Vec<char> = scalar_result.chars().collect();
            prop_assert_eq!(simd_chars, scalar_chars);

            // This will also verify the strings are equal
            prop_assert_eq!(simd_result, scalar_result);
        }
    }
}
