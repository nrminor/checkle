#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_hash_output_to_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.txt");
    let output_file = temp_dir.path().join("output_hashes.txt");

    fs::write(&test_file, b"Hello, World!").expect("Failed to write test file");

    // Run checkle hash with --hash-output
    let mut cmd = Command::cargo_bin("checkle").unwrap();
    cmd.arg("hash")
        .arg(&test_file)
        .arg("--hash-output")
        .arg(&output_file);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("65a8e27d8879283831b664bd8b7f0ad4"));

    // Verify output file exists and contains the hash
    assert!(output_file.exists(), "Output file should exist");
    let contents = fs::read_to_string(&output_file).expect("Failed to read output file");
    assert!(
        contents.contains("65a8e27d8879283831b664bd8b7f0ad4"),
        "Should contain MD5 hash"
    );
    assert!(contents.contains("test.txt"), "Should contain filename");

    // Verify checksum.txt was NOT created
    assert!(
        !temp_dir.path().join("checksum.txt").exists(),
        "checksum.txt should not be created when using --hash-output"
    );
}

#[test]
fn test_hash_output_file_already_exists() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.txt");
    let output_file = temp_dir.path().join("output_hashes.txt");

    fs::write(&test_file, b"Hello, World!").expect("Failed to write test file");
    fs::write(&output_file, b"existing content").expect("Failed to write existing file");

    // Run checkle hash with --hash-output pointing to existing file
    let mut cmd = Command::cargo_bin("checkle").unwrap();
    cmd.arg("hash")
        .arg(&test_file)
        .arg("--hash-output")
        .arg(&output_file);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"))
        .stderr(predicate::str::contains("Please remove it"));

    // Verify the existing file was not modified
    let contents = fs::read_to_string(&output_file).expect("Failed to read output file");
    assert_eq!(
        contents, "existing content",
        "Existing file should not be modified"
    );
}

#[test]
fn test_hash_default_behavior_unchanged() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test.txt");

    fs::write(&test_file, b"Hello, World!").expect("Failed to write test file");

    // Change to temp dir to ensure checksum.txt is created there
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&temp_dir).expect("Failed to change dir");

    // Run checkle hash without --hash-output
    let mut cmd = Command::cargo_bin("checkle").unwrap();
    cmd.arg("hash").arg(&test_file);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("65a8e27d8879283831b664bd8b7f0ad4"))
        .stdout(predicate::str::contains("test.txt"));

    // Verify checksum.txt was NOT created (new default behavior is stdout only)
    assert!(
        !temp_dir.path().join("checksum.txt").exists(),
        "checksum.txt should NOT be created by default (output is to stdout only)"
    );

    // Restore original directory
    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
