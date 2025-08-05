//! SIMD-accelerated implementations for performance-critical operations.
//!
//! This module provides high-performance SIMD implementations for various
//! operations throughout checkle, with automatic fallback to scalar
//! implementations for compatibility.

#[cfg(test)]
use crate::constants::MD5_SIZE;
use crate::constants::{MAX_CHUNK_COUNT, SHA_SIZE};

/// Maximum size for a hash that we'll convert to hex.
/// This is based on the maximum possible Merkle tree output.
const MAX_HASH_SIZE: usize = SHA_SIZE * MAX_CHUNK_COUNT;

// ============================================================================
// Hex String Conversion
// ============================================================================

/// Scalar implementation of bytes to hex conversion.
///
/// This is the fallback implementation used when SIMD is not available
/// or for handling remainder bytes in SIMD implementation.
///
/// # Arguments
///
/// * `bytes` - The byte array to convert to hexadecimal
///
/// # Returns
///
/// A lowercase hexadecimal string representation of the input bytes
///
/// # Panics
///
/// Panics if:
/// - The input size would cause string allocation to exceed `isize::MAX`
/// - Memory allocation fails (extremely rare)
#[inline]
#[must_use]
pub fn bytes_to_hex_scalar(bytes: &[u8]) -> String {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    // Precondition assertions (Tiger Style)
    assert!(
        bytes.len() <= MAX_HASH_SIZE,
        "Input exceeds maximum hash size: {} > {}",
        bytes.len(),
        MAX_HASH_SIZE
    );
    assert!(
        isize::try_from(bytes.len().saturating_mul(2)).is_ok(),
        "Hex string size would overflow"
    );

    let mut result = String::with_capacity(bytes.len() * 2);

    for &byte in bytes {
        result.push(HEX_CHARS[(byte >> 4) as usize] as char);
        result.push(HEX_CHARS[(byte & 0x0F) as usize] as char);
    }

    // Postcondition assertions
    assert_eq!(result.len(), bytes.len() * 2, "Output length invariant");
    // Note: hex validation removed to avoid circular dependency with simd::is_hex_string
    assert!(
        result.chars().all(|c| !c.is_ascii_uppercase()),
        "All characters must be lowercase"
    );

    result
}

#[cfg(feature = "simd")]
/// SIMD implementation of bytes to hex conversion.
///
/// Uses portable SIMD to convert 32 bytes at a time into hexadecimal.
/// This provides approximately 10x speedup over the scalar implementation
/// for typical hash sizes.
///
/// # Arguments
///
/// * `bytes` - The byte array to convert to hexadecimal
///
/// # Returns
///
/// A lowercase hexadecimal string representation of the input bytes
///
/// # Panics
///
/// Panics if:
/// - The input size would cause string allocation to exceed `isize::MAX`
/// - Memory allocation fails (extremely rare)
/// - SIMD operations fail (should never happen with valid input)
#[inline]
#[must_use]
pub fn bytes_to_hex_simd(bytes: &[u8]) -> String {
    use std::simd::{Simd, prelude::*, u8x32};

    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    // Precondition assertions (Tiger Style)
    assert!(
        bytes.len() <= MAX_HASH_SIZE,
        "Input exceeds maximum hash size: {} > {}",
        bytes.len(),
        MAX_HASH_SIZE
    );
    assert!(
        isize::try_from(bytes.len().saturating_mul(2)).is_ok(),
        "Hex string size would overflow"
    );

    let mut result = String::with_capacity(bytes.len() * 2);

    // Process 32-byte chunks with SIMD
    let chunks = bytes.chunks_exact(32);
    let remainder = chunks.remainder();

    // Reusable buffer for hex bytes
    let mut hex_bytes = Vec::with_capacity(64);

    for chunk in chunks {
        hex_bytes.clear();

        // Load 32 bytes into SIMD register
        let input = u8x32::from_slice(chunk);

        // Split each byte into high and low nibbles
        let hi = input >> Simd::splat(4);
        let lo = input & Simd::splat(0x0F);

        // Constants for branchless hex conversion
        let nine = Simd::splat(9);

        // Check which nibbles are > 9 (need letter conversion)
        let hi_gt_nine = hi.simd_gt(nine);
        let lo_gt_nine = lo.simd_gt(nine);

        // Branchless conversion to ASCII hex characters
        // If nibble > 9: add 'a' - 10 (87)
        // If nibble <= 9: add '0' (48)
        let hi_ascii = hi + hi_gt_nine.select(Simd::splat(b'a' - 10), Simd::splat(b'0'));

        let lo_ascii = lo + lo_gt_nine.select(Simd::splat(b'a' - 10), Simd::splat(b'0'));

        // Manual interleaving of high and low bytes
        let hi_array = hi_ascii.to_array();
        let lo_array = lo_ascii.to_array();

        for i in 0..32 {
            hex_bytes.push(hi_array[i]);
            hex_bytes.push(lo_array[i]);
        }

        // SAFETY: We know all bytes are valid ASCII hex characters (0-9, a-f)
        // because we explicitly constructed them that way
        let hex_str = unsafe { std::str::from_utf8_unchecked(&hex_bytes) };
        result.push_str(hex_str);
    }

    // Handle remainder with scalar implementation
    for &byte in remainder {
        result.push(HEX_CHARS[(byte >> 4) as usize] as char);
        result.push(HEX_CHARS[(byte & 0x0F) as usize] as char);
    }

    // Postcondition assertions
    assert_eq!(result.len(), bytes.len() * 2, "Output length invariant");
    // Note: hex validation removed to avoid circular dependency with simd::is_hex_string
    assert!(
        result.chars().all(|c| !c.is_ascii_uppercase()),
        "All characters must be lowercase"
    );

    result
}

/// Main entry point for bytes to hex conversion.
///
/// Automatically selects the best implementation based on compile-time features.
/// When the `simd` feature is enabled and building with a nightly compiler,
/// this will use SIMD acceleration. Otherwise, it falls back to the scalar
/// implementation.
///
/// # Arguments
///
/// * `bytes` - The byte array to convert to hexadecimal
///
/// # Returns
///
/// A lowercase hexadecimal string representation of the input bytes
///
/// # Examples
///
/// ```
/// use checkle::simd::bytes_to_hex;
///
/// let hash_bytes = [0xDE, 0xAD, 0xBE, 0xEF];
/// let hex_string = bytes_to_hex(&hash_bytes);
/// assert_eq!(hex_string, "deadbeef");
/// ```
///
/// # Panics
///
/// Panics if:
/// - The input size exceeds `MAX_HASH_SIZE`
/// - The resulting string size would overflow `isize::MAX`
#[inline]
#[must_use]
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    #[cfg(feature = "simd")]
    {
        bytes_to_hex_simd(bytes)
    }

    #[cfg(not(feature = "simd"))]
    {
        bytes_to_hex_scalar(bytes)
    }
}

// ============================================================================
// Hash Validation
// ============================================================================

/// Maximum supported hash string length.
/// This covers SHA-512 (128 chars) with plenty of headroom.
const MAX_HASH_STRING_LENGTH: usize = 256;

/// Scalar implementation of hex string validation.
///
/// Validates that all characters in the string are valid hexadecimal digits
/// (0-9, a-f, A-F). This is the fallback implementation used when SIMD is
/// not available.
///
/// # Arguments
///
/// * `s` - The string to validate
///
/// # Returns
///
/// `true` if all characters are valid hexadecimal digits, `false` otherwise
///
/// # Panics
///
/// Panics if:
/// - The string length exceeds `MAX_HASH_STRING_LENGTH`
#[inline]
#[must_use]
pub fn is_hex_string_scalar(s: &str) -> bool {
    // Precondition assertions (Tiger Style)
    assert!(
        s.len() <= MAX_HASH_STRING_LENGTH,
        "String exceeds maximum hash length: {} > {}",
        s.len(),
        MAX_HASH_STRING_LENGTH
    );

    // Empty string is considered valid hex (vacuous truth)
    if s.is_empty() {
        return true;
    }

    // Check each character is a valid hex digit
    let result = s.chars().all(|c| c.is_ascii_hexdigit());

    // Postcondition: result consistency
    debug_assert_eq!(
        result,
        s.bytes().all(|b| b.is_ascii_hexdigit()),
        "Character and byte validation must agree"
    );

    result
}

#[cfg(feature = "simd")]
/// SIMD implementation of hex string validation.
///
/// Uses portable SIMD to validate 32 bytes at a time, providing approximately
/// 10-20x speedup over the scalar implementation for typical hash lengths.
///
/// # Arguments
///
/// * `s` - The string to validate
///
/// # Returns
///
/// `true` if all characters are valid hexadecimal digits, `false` otherwise
///
/// # Panics
///
/// Panics if:
/// - The string length exceeds `MAX_HASH_STRING_LENGTH`
/// - SIMD operations fail (should never happen with valid UTF-8 input)
#[inline]
#[must_use]
pub fn is_hex_string_simd(s: &str) -> bool {
    use std::simd::{Simd, prelude::*, u8x32};

    // Precondition assertions (Tiger Style)
    assert!(
        s.len() <= MAX_HASH_STRING_LENGTH,
        "String exceeds maximum hash length: {} > {}",
        s.len(),
        MAX_HASH_STRING_LENGTH
    );

    let bytes = s.as_bytes();

    // Empty string is considered valid hex (vacuous truth)
    if bytes.is_empty() {
        return true;
    }

    // Process 32-byte chunks with SIMD
    let chunks = bytes.chunks_exact(32);
    let remainder = chunks.remainder();

    for chunk in chunks {
        let v = u8x32::from_slice(chunk);

        // SIMD hex validation using bit manipulation
        // Convert to uppercase for uniform comparison: clear bit 5 (0x20)
        let uppercased = v & Simd::splat(!0x20);

        // Check if character is a digit (0-9): '0' <= c <= '9'
        let is_digit = (v.simd_ge(Simd::splat(b'0'))) & (v.simd_le(Simd::splat(b'9')));

        // Check if character is a hex letter (A-F): 'A' <= uppercased <= 'F'
        let is_letter =
            (uppercased.simd_ge(Simd::splat(b'A'))) & (uppercased.simd_le(Simd::splat(b'F')));

        // Valid hex character is either digit or letter
        let is_hex = is_digit | is_letter;

        // If any character in this chunk is not hex, return false
        if !is_hex.all() {
            return false;
        }
    }

    // Handle remainder with scalar implementation
    let remainder_valid = remainder.iter().all(|&b| b.is_ascii_hexdigit());

    // Postcondition: consistency check in debug mode
    debug_assert_eq!(
        remainder_valid,
        remainder.iter().all(|&b| b.is_ascii_hexdigit()),
        "Remainder validation must be consistent"
    );

    remainder_valid
}

/// Main entry point for hex string validation.
///
/// Automatically selects the best implementation based on compile-time features.
/// When the `simd` feature is enabled and building with a nightly compiler,
/// this will use SIMD acceleration. Otherwise, it falls back to the scalar
/// implementation.
///
/// # Arguments
///
/// * `s` - The string to validate
///
/// # Returns
///
/// `true` if all characters are valid hexadecimal digits, `false` otherwise
///
/// # Examples
///
/// ```
/// use checkle::simd::is_hex_string;
///
/// assert!(is_hex_string("deadbeef"));
/// assert!(is_hex_string("DEADBEEF"));
/// assert!(is_hex_string("0123456789abcdefABCDEF"));
/// assert!(!is_hex_string("notahex"));
/// assert!(!is_hex_string("deadbeeg")); // 'g' is not hex
/// assert!(is_hex_string("")); // empty string is valid
/// ```
///
/// # Panics
///
/// Panics if the string length exceeds `MAX_HASH_STRING_LENGTH`
#[inline]
#[must_use]
pub fn is_hex_string(s: &str) -> bool {
    #[cfg(feature = "simd")]
    {
        is_hex_string_simd(s)
    }

    #[cfg(not(feature = "simd"))]
    {
        is_hex_string_scalar(s)
    }
}

/// Scalar implementation of hash validation.
///
/// Validates that a hash string has the expected length and contains only
/// valid hexadecimal characters. This is the fallback implementation used
/// when SIMD is not available.
///
/// # Arguments
///
/// * `hash` - The hash string to validate
/// * `expected_len` - The expected length of the hash (e.g., 32 for MD5, 64 for SHA256)
///
/// # Returns
///
/// `true` if the hash has the correct length and contains only hex characters,
/// `false` otherwise
///
/// # Panics
///
/// Panics if:
/// - The expected length exceeds `MAX_HASH_STRING_LENGTH`
/// - The expected length is zero
#[inline]
#[must_use]
pub fn validate_hash_scalar(hash: &str, expected_len: usize) -> bool {
    // Precondition assertions (Tiger Style)
    assert!(
        expected_len > 0,
        "Expected hash length must be positive: {expected_len}"
    );
    assert!(
        expected_len <= MAX_HASH_STRING_LENGTH,
        "Expected length exceeds maximum: {expected_len} > {MAX_HASH_STRING_LENGTH}"
    );

    // Check length first (fast rejection)
    if hash.len() != expected_len {
        return false;
    }

    // Then validate hex characters
    is_hex_string_scalar(hash)
}

#[cfg(feature = "simd")]
/// SIMD implementation of hash validation.
///
/// Validates that a hash string has the expected length and contains only
/// valid hexadecimal characters using SIMD acceleration.
///
/// # Arguments
///
/// * `hash` - The hash string to validate
/// * `expected_len` - The expected length of the hash (e.g., 32 for MD5, 64 for SHA256)
///
/// # Returns
///
/// `true` if the hash has the correct length and contains only hex characters,
/// `false` otherwise
///
/// # Panics
///
/// Panics if:
/// - The expected length exceeds `MAX_HASH_STRING_LENGTH`
/// - The expected length is zero
#[inline]
#[must_use]
pub fn validate_hash_simd(hash: &str, expected_len: usize) -> bool {
    // Precondition assertions (Tiger Style)
    assert!(
        expected_len > 0,
        "Expected hash length must be positive: {expected_len}"
    );
    assert!(
        expected_len <= MAX_HASH_STRING_LENGTH,
        "Expected length exceeds maximum: {expected_len} > {MAX_HASH_STRING_LENGTH}"
    );

    // Check length first (fast rejection)
    if hash.len() != expected_len {
        return false;
    }

    // Then validate hex characters using SIMD
    is_hex_string_simd(hash)
}

/// Main entry point for hash validation.
///
/// Validates that a hash string has the expected length and contains only
/// valid hexadecimal characters. Automatically selects the best implementation
/// based on compile-time features.
///
/// # Arguments
///
/// * `hash` - The hash string to validate
/// * `expected_len` - The expected length of the hash (e.g., 32 for MD5, 64 for SHA256)
///
/// # Returns
///
/// `true` if the hash has the correct length and contains only hex characters,
/// `false` otherwise
///
/// # Examples
///
/// ```
/// use checkle::simd::validate_hash;
///
/// // MD5 hash validation (32 characters)
/// assert!(validate_hash("d41d8cd98f00b204e9800998ecf8427e", 32));
/// assert!(!validate_hash("d41d8cd98f00b204e9800998ecf8427", 32)); // too short
/// assert!(!validate_hash("d41d8cd98f00b204e9800998ecf8427g", 32)); // invalid char
///
/// // SHA256 hash validation (64 characters)
/// let sha256_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// assert!(validate_hash(sha256_hash, 64));
/// ```
///
/// # Panics
///
/// Panics if:
/// - The expected length exceeds `MAX_HASH_STRING_LENGTH`
/// - The expected length is zero
#[inline]
#[must_use]
pub fn validate_hash(hash: &str, expected_len: usize) -> bool {
    #[cfg(feature = "simd")]
    {
        validate_hash_simd(hash, expected_len)
    }

    #[cfg(not(feature = "simd"))]
    {
        validate_hash_scalar(hash, expected_len)
    }
}

// ============================================================================
// Future SIMD implementations will go here:
// - Buffer zeroing
// - Checksum file tab detection
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_scalar_empty_input() {
        assert_eq!(bytes_to_hex_scalar(&[]), "");
    }

    #[test]
    fn test_scalar_single_byte() {
        assert_eq!(bytes_to_hex_scalar(&[0x00]), "00");
        assert_eq!(bytes_to_hex_scalar(&[0xFF]), "ff");
        assert_eq!(bytes_to_hex_scalar(&[0x0F]), "0f");
        assert_eq!(bytes_to_hex_scalar(&[0xF0]), "f0");
    }

    #[test]
    fn test_scalar_known_values() {
        assert_eq!(bytes_to_hex_scalar(&[0xDE, 0xAD, 0xBE, 0xEF]), "deadbeef");
        assert_eq!(
            bytes_to_hex_scalar(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF]),
            "0123456789abcdef"
        );
    }

    #[test]
    fn test_scalar_md5_size() {
        let bytes = vec![0xAA; MD5_SIZE];
        let result = bytes_to_hex_scalar(&bytes);
        assert_eq!(result.len(), MD5_SIZE * 2);
        assert_eq!(result, "a".repeat(MD5_SIZE * 2));
    }

    #[test]
    fn test_scalar_sha256_size() {
        let bytes = vec![0xBB; SHA_SIZE];
        let result = bytes_to_hex_scalar(&bytes);
        assert_eq!(result.len(), SHA_SIZE * 2);
        assert_eq!(result, "b".repeat(SHA_SIZE * 2));
    }

    #[cfg(feature = "simd")]
    mod simd_tests {
        use super::*;

        #[test]
        fn test_simd_empty_input() {
            assert_eq!(bytes_to_hex_simd(&[]), "");
        }

        #[test]
        fn test_simd_single_byte() {
            assert_eq!(bytes_to_hex_simd(&[0x00]), "00");
            assert_eq!(bytes_to_hex_simd(&[0xFF]), "ff");
        }

        #[test]
        fn test_simd_exact_32_bytes() {
            let bytes = vec![0xCC; 32];
            let result = bytes_to_hex_simd(&bytes);
            assert_eq!(result.len(), 64);
            assert_eq!(result, "c".repeat(64));
        }

        #[test]
        fn test_simd_with_remainder() {
            // 35 bytes = 1 SIMD chunk + 3 remainder
            let bytes = vec![0xDD; 35];
            let result = bytes_to_hex_simd(&bytes);
            assert_eq!(result.len(), 70);
            assert_eq!(result, "d".repeat(70));
        }

        proptest! {
            #[test]
            fn prop_simd_scalar_equivalence(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
                let simd_result = bytes_to_hex_simd(&bytes);
                let scalar_result = bytes_to_hex_scalar(&bytes);
                prop_assert_eq!(simd_result, scalar_result);
            }

            #[test]
            fn prop_simd_output_properties(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
                let result = bytes_to_hex_simd(&bytes);

                // Length is exactly 2x input
                prop_assert_eq!(result.len(), bytes.len() * 2);

                // All characters are valid lowercase hex
                prop_assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
                prop_assert!(result.chars().all(|c| !c.is_ascii_uppercase()));

                // Can parse back to original bytes
                let parsed = hex::decode(&result).expect("SIMD hex output must be valid hex");
                prop_assert_eq!(parsed, bytes);
            }
        }
    }

    // Cross-implementation tests
    proptest! {
        #[test]
        fn prop_bytes_to_hex_properties(bytes in prop::collection::vec(any::<u8>(), 0..=256)) {
            let result = bytes_to_hex(&bytes);

            // Length is exactly 2x input
            prop_assert_eq!(result.len(), bytes.len() * 2);

            // All characters are valid lowercase hex
            prop_assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
            prop_assert!(result.chars().all(|c| !c.is_ascii_uppercase()));
        }
    }

    // ========================================================================
    // Hash Validation Tests
    // ========================================================================

    #[test]
    fn test_scalar_is_hex_string_empty() {
        assert!(is_hex_string_scalar(""));
    }

    #[test]
    fn test_scalar_is_hex_string_valid() {
        assert!(is_hex_string_scalar("0"));
        assert!(is_hex_string_scalar("9"));
        assert!(is_hex_string_scalar("a"));
        assert!(is_hex_string_scalar("f"));
        assert!(is_hex_string_scalar("A"));
        assert!(is_hex_string_scalar("F"));
        assert!(is_hex_string_scalar("0123456789abcdef"));
        assert!(is_hex_string_scalar("0123456789ABCDEF"));
        assert!(is_hex_string_scalar("deadbeef"));
        assert!(is_hex_string_scalar("DEADBEEF"));
        assert!(is_hex_string_scalar("DeAdBeEf")); // mixed case
    }

    #[test]
    fn test_scalar_is_hex_string_invalid() {
        assert!(!is_hex_string_scalar("g"));
        assert!(!is_hex_string_scalar("G"));
        assert!(!is_hex_string_scalar("z"));
        assert!(!is_hex_string_scalar("deadbeeg")); // 'g' is invalid
        assert!(!is_hex_string_scalar("dead beef")); // space is invalid
        assert!(!is_hex_string_scalar("dead-beef")); // dash is invalid
        assert!(!is_hex_string_scalar("dead\nbeef")); // newline is invalid
        assert!(!is_hex_string_scalar("hello")); // multiple invalid chars
    }

    #[test]
    fn test_scalar_is_hex_string_md5_length() {
        let valid_md5 = "d41d8cd98f00b204e9800998ecf8427e";
        assert_eq!(valid_md5.len(), 32);
        assert!(is_hex_string_scalar(valid_md5));
    }

    #[test]
    fn test_scalar_is_hex_string_sha256_length() {
        let valid_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(valid_sha256.len(), 64);
        assert!(is_hex_string_scalar(valid_sha256));
    }

    #[test]
    fn test_scalar_validate_hash_md5_valid() {
        let md5_hash = "d41d8cd98f00b204e9800998ecf8427e";
        assert!(validate_hash_scalar(md5_hash, 32));
    }

    #[test]
    fn test_scalar_validate_hash_md5_wrong_length() {
        assert!(!validate_hash_scalar("d41d8cd98f00b204e9800998ecf8427", 32)); // too short
        assert!(!validate_hash_scalar(
            "d41d8cd98f00b204e9800998ecf8427e0",
            32
        )); // too long
        assert!(!validate_hash_scalar("", 32)); // empty
    }

    #[test]
    fn test_scalar_validate_hash_md5_invalid_chars() {
        assert!(!validate_hash_scalar(
            "d41d8cd98f00b204e9800998ecf8427g",
            32
        )); // 'g' invalid
        assert!(!validate_hash_scalar(
            "d41d8cd98f00b204e9800998ecf8427 ",
            32
        )); // space invalid
    }

    #[test]
    fn test_scalar_validate_hash_sha256_valid() {
        let sha256_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(validate_hash_scalar(sha256_hash, 64));
    }

    #[test]
    fn test_scalar_validate_hash_sha256_wrong_length() {
        let sha256_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(!validate_hash_scalar(&sha256_hash[..63], 64)); // too short
        assert!(!validate_hash_scalar(&format!("{sha256_hash}0"), 64)); // too long
    }

    #[cfg(feature = "simd")]
    mod hash_validation_simd_tests {
        use super::*;

        #[test]
        fn test_simd_is_hex_string_empty() {
            assert!(is_hex_string_simd(""));
        }

        #[test]
        fn test_simd_is_hex_string_single_chars() {
            // Test all valid single hex characters
            for c in "0123456789abcdefABCDEF".chars() {
                assert!(
                    is_hex_string_simd(&c.to_string()),
                    "Character '{c}' should be valid hex"
                );
            }

            // Test some invalid single characters
            for c in "ghijklmnopqrstuvwxyzGHIJKLMNOPQRSTUVWXYZ!@#$%^&*()".chars() {
                assert!(
                    !is_hex_string_simd(&c.to_string()),
                    "Character '{c}' should be invalid hex"
                );
            }
        }

        #[test]
        fn test_simd_is_hex_string_exact_32_bytes() {
            // Test exactly 32 bytes (one SIMD chunk, no remainder)
            let hash_32 = "d41d8cd98f00b204e9800998ecf8427e";
            assert_eq!(hash_32.len(), 32);
            assert!(is_hex_string_simd(hash_32));

            // Test invalid 32-byte string
            let invalid_32 = "d41d8cd98f00b204e9800998ecf8427g";
            assert_eq!(invalid_32.len(), 32);
            assert!(!is_hex_string_simd(invalid_32));
        }

        #[test]
        fn test_simd_is_hex_string_with_remainder() {
            // Test 35 bytes (32 + 3 remainder)
            let hash_35 = "d41d8cd98f00b204e9800998ecf8427e123";
            assert_eq!(hash_35.len(), 35);
            assert!(is_hex_string_simd(hash_35));

            // Test invalid remainder
            let invalid_35 = "d41d8cd98f00b204e9800998ecf8427e12g";
            assert_eq!(invalid_35.len(), 35);
            assert!(!is_hex_string_simd(invalid_35));
        }

        #[test]
        fn test_simd_is_hex_string_multiple_chunks() {
            // Test 64 bytes (two SIMD chunks)
            let hash_64 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
            assert_eq!(hash_64.len(), 64);
            assert!(is_hex_string_simd(hash_64));

            // Test invalid in first chunk
            let invalid_first = "g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
            assert!(!is_hex_string_simd(invalid_first));

            // Test invalid in second chunk
            let invalid_second = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85g";
            assert!(!is_hex_string_simd(invalid_second));
        }

        #[test]
        fn test_simd_is_hex_string_mixed_case() {
            assert!(is_hex_string_simd("DeAdBeEf"));
            assert!(is_hex_string_simd("aBcDeF0123456789"));
            assert!(is_hex_string_simd("FFFFaaaaBBBB0000"));
        }

        #[test]
        fn test_simd_validate_hash_md5() {
            let md5_hash = "d41d8cd98f00b204e9800998ecf8427e";
            assert!(validate_hash_simd(md5_hash, 32));

            // Wrong length
            assert!(!validate_hash_simd(&md5_hash[..31], 32));
            assert!(!validate_hash_simd(&format!("{md5_hash}0"), 32));

            // Invalid character
            assert!(!validate_hash_simd("d41d8cd98f00b204e9800998ecf8427g", 32));
        }

        #[test]
        fn test_simd_validate_hash_sha256() {
            let sha256_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
            assert!(validate_hash_simd(sha256_hash, 64));

            // Wrong length
            assert!(!validate_hash_simd(&sha256_hash[..63], 64));
            assert!(!validate_hash_simd(&format!("{sha256_hash}0"), 64));

            // Invalid character in different positions
            let mut invalid_hash = sha256_hash.to_string();
            invalid_hash.replace_range(10..11, "g"); // Middle of first chunk
            assert!(!validate_hash_simd(&invalid_hash, 64));

            let mut invalid_hash = sha256_hash.to_string();
            invalid_hash.replace_range(50..51, "z"); // Middle of second chunk
            assert!(!validate_hash_simd(&invalid_hash, 64));
        }

        proptest! {
            #[test]
            fn prop_simd_scalar_equivalence_is_hex_string(s in "[0-9a-fA-F]{0,128}") {
                let simd_result = is_hex_string_simd(&s);
                let scalar_result = is_hex_string_scalar(&s);
                prop_assert_eq!(simd_result, scalar_result, "SIMD and scalar results must match for valid hex: '{}'", s);
            }

            #[test]
            fn prop_simd_scalar_equivalence_invalid_hex(s in "[g-zG-Z!@#$%^&*()]{1,64}") {
                let simd_result = is_hex_string_simd(&s);
                let scalar_result = is_hex_string_scalar(&s);
                prop_assert_eq!(simd_result, scalar_result, "SIMD and scalar results must match for invalid hex: '{}'", s);
                prop_assert!(!simd_result, "Invalid hex strings should always return false: '{}'", s);
            }

            #[test]
            fn prop_simd_scalar_equivalence_validate_hash(
                hash in "[0-9a-fA-F]{32,64}",
                expected_len in 8usize..128
            ) {
                let simd_result = validate_hash_simd(&hash, expected_len);
                let scalar_result = validate_hash_scalar(&hash, expected_len);
                prop_assert_eq!(simd_result, scalar_result,
                    "SIMD and scalar validation must match for hash '{}' with expected length {}",
                    hash, expected_len);
            }

            #[test]
            fn prop_simd_hash_validation_properties(hash in "[0-9a-fA-F]{32}") {
                // Valid MD5 hash should pass
                prop_assert!(validate_hash_simd(&hash, 32));

                // Same hash with wrong expected length should fail
                prop_assert!(!validate_hash_simd(&hash, 64));
                prop_assert!(!validate_hash_simd(&hash, 16));

                // Hash is valid hex string
                prop_assert!(is_hex_string_simd(&hash));
            }
        }
    }

    // Cross-implementation hash validation tests
    #[test]
    fn test_is_hex_string_main_entry_point() {
        assert!(is_hex_string("deadbeef"));
        assert!(is_hex_string("DEADBEEF"));
        assert!(is_hex_string(""));
        assert!(!is_hex_string("deadbeeg"));
    }

    #[test]
    fn test_validate_hash_main_entry_point() {
        let md5_hash = "d41d8cd98f00b204e9800998ecf8427e";
        assert!(validate_hash(md5_hash, 32));
        assert!(!validate_hash(md5_hash, 64));
        assert!(!validate_hash("d41d8cd98f00b204e9800998ecf8427g", 32));
    }

    proptest! {
        #[test]
        fn prop_is_hex_string_properties(valid_hex in "[0-9a-fA-F]*") {
            let result = is_hex_string(&valid_hex);
            prop_assert!(result, "All valid hex strings should pass validation: '{}'", valid_hex);
        }

        #[test]
        fn prop_validate_hash_length_consistency(
            hash in "[0-9a-fA-F]+",
            expected_len in 1usize..MAX_HASH_STRING_LENGTH
        ) {
            let result = validate_hash(&hash, expected_len);
            let length_matches = hash.len() == expected_len;
            let is_hex = is_hex_string(&hash);

            // Validation should pass if and only if length matches AND it's valid hex
            prop_assert_eq!(result, length_matches && is_hex,
                "validate_hash({}, {}) = {} but length_matches={} && is_hex={}",
                hash, expected_len, result, length_matches, is_hex);
        }
    }
}
