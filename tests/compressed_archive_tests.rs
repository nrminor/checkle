//! Tests for compressed archive introspection (Phase 1 fix verification)
//!
//! These tests verify that the critical decompression bug is fixed and that
//! compressed TAR archives (.tar.gz, .tar.bz2, .tar.xz) now produce correct hashes.

use assert_cmd::Command;
use md5::{Digest, Md5};
use predicates::prelude::*;
use std::fs;
use std::path::Path;

/// Expected MD5 hash for the test file content "test content for archives\n"
const EXPECTED_TEST_FILE_HASH: &str = "e19d269d1757cacb8626365b51a87330";

#[test]
fn test_tar_gz_decompression_correctness() {
    // Ensure test archive exists
    let archive_path = "tests/data/test_archive.tar.gz";
    assert!(
        Path::new(archive_path).exists(),
        "Test archive {archive_path} must exist"
    );

    // Test that checkle can correctly hash a file inside a .tar.gz archive
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:test_file.txt"))
        .arg("--algorithm")
        .arg("md5");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_TEST_FILE_HASH));
}

#[test]
fn test_tar_bz2_decompression_correctness() {
    // Ensure test archive exists
    let archive_path = "tests/data/test_archive.tar.bz2";
    assert!(
        Path::new(archive_path).exists(),
        "Test archive {archive_path} must exist"
    );

    // Test that checkle can correctly hash a file inside a .tar.bz2 archive
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:test_file.txt"))
        .arg("--algorithm")
        .arg("md5");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_TEST_FILE_HASH));
}

#[test]
fn test_tar_xz_decompression_correctness() {
    // Ensure test archive exists
    let archive_path = "tests/data/test_archive.tar.xz";
    assert!(
        Path::new(archive_path).exists(),
        "Test archive {archive_path} must exist"
    );

    // Test that checkle can correctly hash a file inside a .tar.xz archive
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:test_file.txt"))
        .arg("--algorithm")
        .arg("md5");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_TEST_FILE_HASH));
}

#[test]
fn test_plain_tar_still_works() {
    // Ensure test archive exists
    let archive_path = "tests/data/test_archive.tar";
    assert!(
        Path::new(archive_path).exists(),
        "Test archive {archive_path} must exist"
    );

    // Test that plain TAR archives still work after decompression changes
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:test_file.txt"))
        .arg("--algorithm")
        .arg("md5");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(EXPECTED_TEST_FILE_HASH));
}

#[test]
fn test_archive_without_colon_hashed_as_file() {
    // Test that archives without ':' are hashed as regular files (Phase 0 fix)
    let archive_path = "tests/data/test_archive.tar.gz";
    assert!(
        Path::new(archive_path).exists(),
        "Test archive {archive_path} must exist"
    );

    // Get the expected hash of the archive file itself
    let archive_contents = fs::read(archive_path).expect("Failed to read archive");
    let mut hasher = Md5::new();
    hasher.update(&archive_contents);
    let expected_hash = format!("{:x}", hasher.finalize());

    // Test that checkle hashes the archive file itself without ':'
    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(archive_path)
        .arg("--algorithm")
        .arg("md5");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(expected_hash));
}

#[test]
fn test_nonexistent_file_in_compressed_archive() {
    // Test proper error handling for non-existent files in compressed archives
    let archive_path = "tests/data/test_archive.tar.gz";

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:nonexistent.txt"))
        .arg("--algorithm")
        .arg("md5");

    cmd.assert().failure().stderr(
        predicate::str::contains("not found").or(predicate::str::contains("Entry not found")),
    );
}

#[test]
fn test_sha256_with_compressed_archive() {
    // Test SHA256 algorithm with compressed archives
    let archive_path = "tests/data/test_archive.tar.gz";

    let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
    cmd.arg("hash")
        .arg(format!("{archive_path}:test_file.txt"))
        .arg("--algorithm")
        .arg("sha2");

    // We just verify it succeeds and produces a SHA256 hash (64 hex chars)
    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"^[a-f0-9]{64}\t").expect("Invalid regex pattern"));
}
