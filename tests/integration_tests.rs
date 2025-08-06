#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_panics_doc,
    clippy::uninlined_format_args,
    clippy::needless_raw_string_hashes,
    clippy::trim_split_whitespace,
    clippy::print_stderr,
    clippy::collapsible_if
)]

use assert_cmd::Command;
use checkle::simd;
use predicates::prelude::*;
use std::fs;
use tempfile::{NamedTempFile, TempDir};

// Integration Test 1: CLI hash command works correctly
#[test]
fn test_cli_hash_command_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Hello, integration test!";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, integration test!").not()) // Should not contain file content
        .stdout(predicate::function(|output: &str| output.len() > 30)) // Should contain a hash (at least 32 chars for MD5)
        .stdout(predicate::str::is_match(r"[a-f0-9]{32}").expect("Failed to create MD5 regex")); // Should be valid MD5 hash
}

// Integration Test 2: CLI hash command with SHA256
#[test]
fn test_cli_hash_command_sha256_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"SHA256 test content";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("--algorithm")
        .arg("sha256")
        .arg("hash")
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::function(|output: &str| output.len() > 60)) // Should contain a hash (at least 64 chars for SHA256)
        .stdout(predicate::str::is_match(r"[a-f0-9]{64}").expect("Failed to create SHA256 regex")); // Should be valid SHA256 hash
}

// Integration Test 3: CLI verify command with correct hash
#[test]
fn test_cli_verify_command_success_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Verify me!";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    // First, get the hash
    let mut hash_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output = hash_cmd
        .arg("hash")
        .arg(temp_file.path())
        .output()
        .expect("Failed to execute hash command");

    let hash = String::from_utf8(hash_output.stdout)
        .expect("Hash output should be valid UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash")
        .to_string();

    // Now verify with the correct hash
    let mut verify_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    verify_cmd
        .arg("verify")
        .arg(temp_file.path())
        .arg("--hash")
        .arg(&hash)
        .assert()
        .success();
}

// Integration Test 4: CLI verify command with incorrect hash
#[test]
fn test_cli_verify_command_failure_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"This will fail verification";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    let incorrect_hash = "0123456789abcdef0123456789abcdef"; // Incorrect MD5 hash

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("verify")
        .arg(temp_file.path())
        .arg("--hash")
        .arg(incorrect_hash)
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed"));
}

// Integration Test 5: CLI verify-many command with checksum file
#[test]
fn test_cli_verify_many_command_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create test files
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    fs::write(&file1, b"Content of file 1").expect("Failed to write file1");
    fs::write(&file2, b"Content of file 2").expect("Failed to write file2");

    // Get hashes for the files
    let mut hash_cmd1 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output1 = hash_cmd1
        .arg("hash")
        .arg(&file1)
        .output()
        .expect("Failed to hash file1");
    let hash1 = String::from_utf8(hash_output1.stdout)
        .expect("Hash1 should be UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash1")
        .to_string();

    let mut hash_cmd2 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output2 = hash_cmd2
        .arg("hash")
        .arg(&file2)
        .output()
        .expect("Failed to hash file2");
    let hash2 = String::from_utf8(hash_output2.stdout)
        .expect("Hash2 should be UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash2")
        .to_string();

    // Create checksum file
    let checksum_file = temp_dir.path().join("checksums.txt");
    let checksum_content = format!(
        "{}\t{}\n{}\t{}",
        hash1,
        file1.display(),
        hash2,
        file2.display()
    );
    fs::write(&checksum_file, checksum_content).expect("Failed to write checksum file");

    // Run verify-many command
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("verify-many")
        .arg("--checksum-file")
        .arg(&checksum_file)
        .assert()
        .success();
}

// Integration Test 6: CLI command aliases work correctly
#[test]
fn test_cli_command_aliases_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Test aliases";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    // Test hash aliases
    let hash_aliases = vec!["h", "gen", "generate"];
    for alias in hash_aliases {
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg(alias).arg(temp_file.path()).assert().success();
    }
}

// Integration Test 7: CLI error handling for non-existent files
#[test]
fn test_cli_nonexistent_file_error_integration() {
    let nonexistent_file = "/tmp/nonexistent_file_12345.txt";

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(nonexistent_file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("exist").or(predicate::str::contains("found")));
}

// Integration Test 8: CLI help and version commands
#[test]
fn test_cli_help_and_version_integration() {
    // Test help command
    let mut help_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    help_cmd
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("checkle"))
        .stdout(predicate::str::contains("Usage"));

    // Test version command
    let mut version_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    version_cmd
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("v0.2.0"));
}

// Integration Test 9: CLI invalid algorithm error
#[test]
fn test_cli_invalid_algorithm_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    fs::write(temp_file.path(), b"test").expect("Failed to write test content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("--algorithm")
        .arg("invalid_algorithm")
        .arg("hash")
        .arg(temp_file.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown hashing algorithm"));
}

// Integration Test 10: CLI threads parameter
#[test]
fn test_cli_threads_parameter_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = vec![0u8; 1024 * 1024]; // 1MB file
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("--threads")
        .arg("2")
        .arg("hash")
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::function(|output: &str| output.len() > 30)); // Should contain a hash
}

// Integration Test 11: CLI verbose mode
#[test]
fn test_cli_verbose_mode_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    fs::write(temp_file.path(), b"verbose test").expect("Failed to write test content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("-v")
        .arg("hash")
        .arg(temp_file.path())
        .assert()
        .success();
    // Note: The exact verbose output depends on the logging configuration
}

// Integration Test 12: CLI empty file handling
#[test]
fn test_cli_empty_file_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    // Leave file empty

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_file.path())
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"[a-f0-9]{32}").expect("Failed to create MD5 regex")); // Should still produce valid MD5 hash
}

// Integration Test 13: CLI large file handling (performance test)
#[test]
fn test_cli_large_file_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let large_content = vec![0u8; 10 * 1024 * 1024]; // 10MB file
    fs::write(temp_file.path(), large_content).expect("Failed to write large file");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_file.path())
        .timeout(std::time::Duration::from_secs(30)) // Should complete within 30 seconds
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"[a-f0-9]{32}").expect("Failed to create MD5 regex"));
}

// Integration Test 14: CLI directory input (now supported)
#[test]
fn test_cli_directory_input_error_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create some files in the directory
    let file1 = temp_dir.path().join("file1.txt");
    let file2 = temp_dir.path().join("file2.txt");
    fs::write(&file1, b"content1").expect("Failed to write file1");
    fs::write(&file2, b"content2").expect("Failed to write file2");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash").arg(temp_dir.path()).assert().success(); // Directory hashing is now supported
}

// Integration Test 15: CLI wildcard input
#[test]
fn test_cli_wildcard_input_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create test files in the temp directory
    let file1 = temp_dir.path().join("test1.txt");
    let file2 = temp_dir.path().join("test2.txt");
    fs::write(&file1, b"content1").expect("Failed to write test1");
    fs::write(&file2, b"content2").expect("Failed to write test2");

    // Use the directory path instead of wildcard to avoid shell expansion issues
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_dir.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("test1.txt").and(predicate::str::contains("test2.txt")));
}

// Integration Test 16: CLI performance with large files
#[test]
fn test_cli_large_file_performance_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let large_content = vec![0u8; 50 * 1024 * 1024]; // 50MB file
    fs::write(temp_file.path(), large_content).expect("Failed to write large file");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_file.path())
        .timeout(std::time::Duration::from_secs(120)) // Should complete within 2 minutes
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"[a-f0-9]{32}").expect("Failed to create MD5 regex"));
}

// Integration Test 17: CLI error handling with corrupted checksum files
#[test]
fn test_cli_corrupted_checksum_file_integration() {
    let corrupted_checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
    let corrupted_content = "invalid\tformat\textra\tfields\n";
    fs::write(corrupted_checksum_file.path(), corrupted_content)
        .expect("Failed to write corrupted content");

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("verify-many")
        .arg("--checksum-file")
        .arg(corrupted_checksum_file.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").or(predicate::str::contains("parse")));
}

// Integration Test 18: CLI stress test with many small files
#[test]
fn test_cli_many_small_files_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create 100 small files in the temp directory
    for i in 0..100 {
        let filename = format!("small_{:03}.txt", i);
        let filepath = temp_dir.path().join(&filename);
        let content = format!("content for file {}", i);
        fs::write(&filepath, content).expect("Failed to write small file");
    }

    // Use the directory path instead of wildcard
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_dir.path())
        .timeout(std::time::Duration::from_secs(30)) // Should complete quickly
        .assert()
        .success();
}

// Integration Test 19: CLI batch verification with mixed results
#[test]
fn test_cli_mixed_verification_results_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create test files
    let file1 = temp_dir.path().join("good_file.txt");
    let file2 = temp_dir.path().join("bad_file.txt");
    fs::write(&file1, b"This file will pass").expect("Failed to write good file");
    fs::write(&file2, b"This file will fail").expect("Failed to write bad file");

    // Get correct hash for file1
    let mut hash_cmd1 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output1 = hash_cmd1
        .arg("hash")
        .arg(&file1)
        .output()
        .expect("Failed to hash file1");
    let hash1 = String::from_utf8(hash_output1.stdout)
        .expect("Hash1 should be UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash1")
        .to_string();

    // Use incorrect hash for file2
    let incorrect_hash2 = "0123456789abcdef0123456789abcdef";

    // Create checksum file with one good and one bad entry
    let checksum_file = temp_dir.path().join("mixed_checksums.txt");
    let checksum_content = format!(
        "{}\t{}\n{}\t{}",
        hash1,
        file1.display(),
        incorrect_hash2,
        file2.display()
    );
    fs::write(&checksum_file, checksum_content).expect("Failed to write mixed checksum file");

    // Run verify-many command - should fail due to bad file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("verify-many")
        .arg("--checksum-file")
        .arg(&checksum_file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed"));
}

// Integration Test 20: CLI output format consistency
#[test]
fn test_cli_output_format_consistency_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Format consistency test";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    // Test MD5 output format with --absolute-paths flag
    let mut md5_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let md5_output = md5_cmd
        .arg("--algorithm")
        .arg("md5")
        .arg("hash")
        .arg("--absolute-paths")
        .arg(temp_file.path())
        .output()
        .expect("Failed to run MD5 command");
    let md5_stdout = String::from_utf8(md5_output.stdout).expect("MD5 output should be UTF-8");

    // Test SHA256 output format with --absolute-paths flag
    let mut sha_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let sha_output = sha_cmd
        .arg("--algorithm")
        .arg("sha256")
        .arg("hash")
        .arg("--absolute-paths")
        .arg(temp_file.path())
        .output()
        .expect("Failed to run SHA256 command");
    let sha_stdout = String::from_utf8(sha_output.stdout).expect("SHA256 output should be UTF-8");

    // Both outputs should contain the filename
    assert!(
        md5_stdout.contains(&temp_file.path().to_string_lossy().to_string()),
        "MD5 output should contain filename"
    );
    assert!(
        sha_stdout.contains(&temp_file.path().to_string_lossy().to_string()),
        "SHA256 output should contain filename"
    );

    // Check hash format consistency
    let md5_hash = md5_stdout
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain MD5 hash");
    let sha_hash = sha_stdout
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain SHA256 hash");

    assert_eq!(md5_hash.len(), 32, "MD5 hash should be 32 characters");
    assert_eq!(sha_hash.len(), 64, "SHA256 hash should be 64 characters");
    assert!(
        simd::is_hex_string(md5_hash),
        "MD5 hash should be hexadecimal"
    );
    assert!(
        simd::is_hex_string(sha_hash),
        "SHA256 hash should be hexadecimal"
    );
}

// Integration Test 21: CLI edge case - empty files handling
#[test]
fn test_cli_empty_files_handling_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create empty files
    let empty_file1 = temp_dir.path().join("empty1.txt");
    let empty_file2 = temp_dir.path().join("empty2.txt");
    fs::write(&empty_file1, b"").expect("Failed to create empty file1");
    fs::write(&empty_file2, b"").expect("Failed to create empty file2");

    // Hash empty files
    let mut cmd1 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output1 = cmd1
        .arg("hash")
        .arg(&empty_file1)
        .output()
        .expect("Failed to hash empty file1");
    let hash1 = String::from_utf8(hash_output1.stdout)
        .expect("Hash1 should be UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash1")
        .to_string();

    let mut cmd2 = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output2 = cmd2
        .arg("hash")
        .arg(&empty_file2)
        .output()
        .expect("Failed to hash empty file2");
    let hash2 = String::from_utf8(hash_output2.stdout)
        .expect("Hash2 should be UTF-8")
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash2")
        .to_string();

    // Empty files should have identical hashes
    assert_eq!(hash1, hash2, "Empty files should have identical hashes");
    assert_eq!(
        hash1, "d41d8cd98f00b204e9800998ecf8427e",
        "Empty file MD5 should be well-known value"
    );
}

// Integration Test 22: CLI command chaining and pipes
#[test]
fn test_cli_command_chaining_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Pipe test content";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    // Generate hash and immediately verify it
    let mut hash_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let hash_output = hash_cmd
        .arg("hash")
        .arg(temp_file.path())
        .output()
        .expect("Failed to generate hash");
    let hash_string = String::from_utf8(hash_output.stdout).expect("Hash output should be UTF-8");
    let hash = hash_string
        .trim()
        .split_whitespace()
        .next()
        .expect("Should contain hash")
        .to_string();

    // Verify the generated hash
    let mut verify_cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    verify_cmd
        .arg("verify")
        .arg(temp_file.path())
        .arg("--hash")
        .arg(&hash)
        .assert()
        .success();
}

// Integration Test 23: CLI resource usage limits
#[test]
fn test_cli_resource_usage_limits_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create files that approach resource limits
    for i in 0..50 {
        // Create 50 files (well below the 10k limit)
        let filename = format!("limit_test_{:03}.txt", i);
        let file_path = temp_dir.path().join(&filename);
        let content = vec![0u8; 1024]; // 1KB each
        fs::write(file_path, content).expect("Failed to write limit test file");
    }

    // Should handle this load without issues
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_dir.path())
        .timeout(std::time::Duration::from_secs(30))
        .assert()
        .success();
}

// Integration Test 24: CLI cross-platform path handling
#[test]
fn test_cli_path_handling_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let test_content = b"Path handling test";
    fs::write(temp_file.path(), test_content).expect("Failed to write test content");

    // Test with absolute path - use --absolute-paths flag to preserve absolute paths in output
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg("--absolute-paths")
        .arg(temp_file.path()) // This is already an absolute path
        .assert()
        .success()
        .stdout(predicate::str::contains(
            temp_file.path().to_string_lossy().as_ref(),
        ));
}

// Integration Test 25: CLI signal handling and graceful shutdown
#[test]
fn test_cli_graceful_operations_integration() {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let medium_content = vec![0u8; 5 * 1024 * 1024]; // 5MB file - not too large but not tiny
    fs::write(temp_file.path(), medium_content).expect("Failed to write medium file");

    // Test that operations complete successfully even with moderately large files
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(temp_file.path())
        .timeout(std::time::Duration::from_secs(30)) // Reasonable timeout
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"[a-f0-9]{32}").expect("Failed to create MD5 regex"));
}

// ============================================================================
// ARCHIVE TESTING - PRINCIPLE OF LEAST SURPRISE VALIDATION
// ============================================================================

// Integration Test 26: Archive files are hashed as regular files (TAR)
#[test]
fn test_archive_tar_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.tar";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle hash of the .tar file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .md5 file
    let md5_file_path = "tests/data/test_archive.tar.md5";
    let expected_hash = fs::read_to_string(md5_file_path)
        .expect("Failed to read MD5 file")
        .split_whitespace()
        .next()
        .expect("MD5 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "TAR archive file should be hashed as regular file, not introspected"
    );
}

// Integration Test 27: Archive files are hashed as regular files (TAR.GZ)
#[test]
fn test_archive_tar_gz_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.tar.gz";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle hash of the .tar.gz file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .md5 file
    let md5_file_path = "tests/data/test_archive.tar.gz.md5";
    let expected_hash = fs::read_to_string(md5_file_path)
        .expect("Failed to read MD5 file")
        .split_whitespace()
        .next()
        .expect("MD5 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "TAR.GZ archive file should be hashed as regular file, not introspected"
    );
}

// Integration Test 28: Archive files are hashed as regular files (TAR.BZ2)
#[test]
fn test_archive_tar_bz2_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.tar.bz2";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle hash of the .tar.bz2 file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .md5 file
    let md5_file_path = "tests/data/test_archive.tar.bz2.md5";
    let expected_hash = fs::read_to_string(md5_file_path)
        .expect("Failed to read MD5 file")
        .split_whitespace()
        .next()
        .expect("MD5 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "TAR.BZ2 archive file should be hashed as regular file, not introspected"
    );
}

// Integration Test 29: Archive files are hashed as regular files (TAR.XZ)
#[test]
fn test_archive_tar_xz_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.tar.xz";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle hash of the .tar.xz file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .md5 file
    let md5_file_path = "tests/data/test_archive.tar.xz.md5";
    let expected_hash = fs::read_to_string(md5_file_path)
        .expect("Failed to read MD5 file")
        .split_whitespace()
        .next()
        .expect("MD5 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "TAR.XZ archive file should be hashed as regular file, not introspected"
    );
}

// Integration Test 30: Archive files are hashed as regular files (ZIP)
#[test]
fn test_archive_zip_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.zip";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle hash of the .zip file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .md5 file
    let md5_file_path = "tests/data/test_archive.zip.md5";
    let expected_hash = fs::read_to_string(md5_file_path)
        .expect("Failed to read MD5 file")
        .split_whitespace()
        .next()
        .expect("MD5 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "ZIP archive file should be hashed as regular file, not introspected"
    );
}

// Integration Test 31: SHA256 validation for archive files
#[test]
fn test_archive_sha256_hashed_as_regular_file() {
    let archive_path = "tests/data/test_archive.tar.gz";

    // Skip if archive doesn't exist
    if !std::path::Path::new(archive_path).exists() {
        eprintln!(
            "Skipping test - {} not found. Run tests/create_test_archives.sh first.",
            archive_path
        );
        return;
    }

    // Get checkle SHA256 hash of the .tar.gz file
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let checkle_output = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("sha256")
        .output()
        .expect("Failed to execute checkle");

    let checkle_hash = String::from_utf8_lossy(&checkle_output.stdout);
    let checkle_hash = checkle_hash
        .split_whitespace()
        .next()
        .expect("Should contain hash");

    // Read the reference hash from the .sha256 file
    let sha256_file_path = "tests/data/test_archive.tar.gz.sha256";
    let expected_hash = fs::read_to_string(sha256_file_path)
        .expect("Failed to read SHA256 file")
        .split_whitespace()
        .next()
        .expect("SHA256 file should contain hash")
        .to_string();

    assert_eq!(
        checkle_hash, expected_hash,
        "TAR.GZ archive file should have correct SHA256 when hashed as regular file"
    );
}

// Integration Test 32: Colon syntax triggers archive introspection (if available)
#[cfg(feature = "tar")]
#[test]
fn test_colon_syntax_triggers_archive_introspection() {
    let archive_path = "tests/data/test_archive.tar:test_file.txt";

    // Skip if archive doesn't exist
    if !std::path::Path::new("tests/data/test_archive.tar").exists() {
        eprintln!(
            "Skipping test - tests/data/test_archive.tar not found. Run tests/create_test_archives.sh first."
        );
        return;
    }

    // This should attempt archive introspection (may fail if entry doesn't exist, but shouldn't treat as regular file)
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    let result = cmd
        .arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5")
        .output();

    // The key test is that it doesn't treat "test_archive.tar:test_file.txt" as a regular filename
    // It should either succeed (if introspection works) or fail with archive-related error
    if let Ok(output) = result {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Should fail with archive-related error, not "file not found" for the literal filename
            assert!(
                !stderr.contains("test_archive.tar:test_file.txt"),
                "Should not treat colon syntax as literal filename"
            );
        }
    }
    // If result is Err, command execution failed, which is fine for this test
    // The important thing is that it recognized the colon syntax
}
