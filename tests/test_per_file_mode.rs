//! Tests for per-file hash mode functionality.
//!
//! This module tests the `--per-file` flag functionality for both
//! hash generation and verification operations.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::{NamedTempFile, TempDir};

#[cfg(test)]
mod per_file_mode_tests {
    use super::*;

    // Test 1: Normal operation - hash with --per-file creates .md5 file
    #[test]
    fn test_hash_per_file_creates_md5_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Test content for per-file mode";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .success();

        // Check that .md5 file was created
        let md5_path = format!("{}.md5", temp_file.path().display());
        assert!(
            std::path::Path::new(&md5_path).exists(),
            "MD5 file should exist at {md5_path}"
        );

        // Read and verify the hash file contains a valid MD5 hash with filename
        let hash_content = fs::read_to_string(&md5_path).expect("Failed to read MD5 file");
        let parts: Vec<&str> = hash_content.trim().split("  ").collect();
        assert_eq!(parts.len(), 2, "Hash file should contain 'hash  filename'");

        let hash = parts[0];
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hexadecimal"
        );
    }

    // Test 2: Normal operation - hash with --per-file and SHA256
    #[test]
    fn test_hash_per_file_creates_sha256_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Test content for SHA256 per-file mode";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("--algorithm")
            .arg("sha256")
            .arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .success();

        // Check that .sha256 file was created
        let sha256_path = format!("{}.sha256", temp_file.path().display());
        assert!(
            std::path::Path::new(&sha256_path).exists(),
            "SHA256 file should exist at {sha256_path}"
        );

        // Read and verify the hash file contains a valid SHA256 hash with filename
        let hash_content = fs::read_to_string(&sha256_path).expect("Failed to read SHA256 file");
        let parts: Vec<&str> = hash_content.trim().split("  ").collect();
        assert_eq!(parts.len(), 2, "Hash file should contain 'hash  filename'");

        let hash = parts[0];
        assert_eq!(hash.len(), 64, "SHA256 hash should be 64 characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hexadecimal"
        );
    }

    // Test 3: Normal operation - verify with --per-file reads from file
    #[test]
    fn test_verify_per_file_reads_from_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Content to verify";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // First, generate the hash with --per-file
        let mut hash_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        hash_cmd
            .arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .success();

        // Now verify using --per-file
        let mut verify_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        verify_cmd
            .arg("verify")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .success();
    }

    // Test 4: Error path - verify with --per-file when hash file doesn't exist
    #[test]
    fn test_verify_per_file_missing_hash_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Content without hash file";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .failure()
            .stderr(predicate::str::contains("does not exist"));
    }

    // Test 5: Normal operation - verify-many with --per-file
    #[test]
    fn test_verify_many_per_file_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create test files
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, b"Content of file 1").expect("Failed to write file1");
        fs::write(&file2, b"Content of file 2").expect("Failed to write file2");

        // Generate hashes with --per-file
        let mut hash_cmd1 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        hash_cmd1
            .arg("hash")
            .arg(&file1)
            .arg("--per-file")
            .assert()
            .success();

        let mut hash_cmd2 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        hash_cmd2
            .arg("hash")
            .arg(&file2)
            .arg("--per-file")
            .assert()
            .success();

        // Verify both files using verify-many --per-file
        let mut verify_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        verify_cmd
            .arg("verify-many")
            .arg("--per-file")
            .arg(&file1)
            .arg(&file2)
            .assert()
            .success();
    }

    // Test 6: Error path - CLI conflicts with --per-file and -o
    #[test]
    fn test_cli_conflicts_per_file_with_output() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .arg("-o")
            .arg("output.txt")
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }

    // Test 7: Error path - CLI conflicts with --per-file and --format
    #[test]
    fn test_cli_conflicts_per_file_with_format() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .arg("--format")
            .arg("json")
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }

    // Test 8: Normal operation - hash multiple files with --per-file
    #[test]
    fn test_hash_multiple_files_per_file_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create test files
        let file1 = temp_dir.path().join("test1.txt");
        let file2 = temp_dir.path().join("test2.txt");
        fs::write(&file1, b"content1").expect("Failed to write test1");
        fs::write(&file2, b"content2").expect("Failed to write test2");

        // Hash each file with --per-file
        let mut cmd1 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd1.arg("hash")
            .arg(&file1)
            .arg("--per-file")
            .assert()
            .success();

        let mut cmd2 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd2.arg("hash")
            .arg(&file2)
            .arg("--per-file")
            .assert()
            .success();

        // Check that hash files were created
        let hash1_path = format!("{}.md5", file1.display());
        let hash2_path = format!("{}.md5", file2.display());
        assert!(
            std::path::Path::new(&hash1_path).exists(),
            "test1.txt.md5 should exist"
        );
        assert!(
            std::path::Path::new(&hash2_path).exists(),
            "test2.txt.md5 should exist"
        );
    }

    // Test 9: Edge case - verify with corrupted hash file
    #[test]
    fn test_verify_per_file_corrupted_hash_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Content to verify";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // Create a corrupted hash file
        let md5_path = format!("{}.md5", temp_file.path().display());
        fs::write(&md5_path, "not_a_valid_hash").expect("Failed to write corrupted hash");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .failure();
    }

    // Test 10: Normal operation - verify-many with mixed results in per-file mode
    #[test]
    fn test_verify_many_per_file_mixed_results() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create test files
        let good_file = temp_dir.path().join("good.txt");
        let bad_file = temp_dir.path().join("bad.txt");
        fs::write(&good_file, b"Good content").expect("Failed to write good file");
        fs::write(&bad_file, b"Bad content").expect("Failed to write bad file");

        // Generate hash for good file
        let mut hash_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        hash_cmd
            .arg("hash")
            .arg(&good_file)
            .arg("--per-file")
            .assert()
            .success();

        // Create incorrect hash for bad file
        let bad_hash_path = format!("{}.md5", bad_file.display());
        fs::write(&bad_hash_path, "00000000000000000000000000000000\n")
            .expect("Failed to write bad hash");

        // Verify both files - should fail due to bad file
        let mut verify_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        verify_cmd
            .arg("verify-many")
            .arg("--per-file")
            .arg(&good_file)
            .arg(&bad_file)
            .assert()
            .failure()
            .stderr(predicate::str::contains("failed"));
    }

    // Test 11: Edge case - empty file with per-file mode
    #[test]
    fn test_hash_empty_file_per_file_mode() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        // Leave file empty

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_file.path())
            .arg("--per-file")
            .assert()
            .success();

        // Check hash file exists and contains the MD5 of empty file
        let md5_path = format!("{}.md5", temp_file.path().display());
        let hash_content = fs::read_to_string(&md5_path).expect("Failed to read MD5 file");
        let parts: Vec<&str> = hash_content.trim().split("  ").collect();
        assert_eq!(parts.len(), 2, "Hash file should contain 'hash  filename'");
        assert_eq!(
            parts[0], "d41d8cd98f00b204e9800998ecf8427e",
            "Empty file MD5 should match"
        );
    }

    // Test 12: Normal operation - verify with --per-file and --hash should conflict
    #[test]
    fn test_verify_conflicts_per_file_with_hash() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_file.path())
            .arg("--per-file")
            .arg("--hash")
            .arg("abc123")
            .assert()
            .failure()
            .stderr(predicate::str::contains("cannot be used with"));
    }
}
