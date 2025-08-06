//! Integration tests for archive functionality with checkle CLI.
//!
//! These tests verify that archive support integrates properly with
//! the main checkle functionality, including CLI commands, progress
//! reporting, and output formatting.

#[cfg(test)]
mod integration_tests {
    use assert_cmd::Command;
    use predicates::prelude::*;
    use std::{
        fs::{self, File},
        io::Write,
        path::{Path, PathBuf},
    };
    use tempfile::TempDir;

    /// Helper to create test files.
    fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent directory");
        }
        let mut file = File::create(&path).expect("Failed to create test file");
        file.write_all(content)
            .expect("Failed to write test file content");
        path
    }

    /// Helper to create a TAR archive using system tar command.
    #[cfg(feature = "tar")]
    fn create_system_tar(dir: &Path, archive_name: &str, files: &[&str]) -> PathBuf {
        let archive_path = dir.join(archive_name);

        // Use system tar command for more realistic test archives
        let mut cmd = std::process::Command::new("tar");
        cmd.arg("-cf").arg(&archive_path).arg("-C").arg(dir);

        for file in files {
            cmd.arg(file);
        }

        let output = cmd.output().expect("Failed to execute tar command");
        assert!(output.status.success(), "tar command failed: {output:?}");

        archive_path
    }

    /// Helper to create a ZIP archive using system zip command.
    #[cfg(feature = "zip")]
    fn create_system_zip(dir: &Path, archive_name: &str, files: &[&str]) -> PathBuf {
        let archive_path = dir.join(archive_name);

        // On Windows, use PowerShell Compress-Archive instead of zip command
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("powershell");
            cmd.arg("-Command");

            let files_list = files.join(", ");
            let ps_command = format!(
                "Compress-Archive -Path {} -DestinationPath '{}'",
                files_list,
                archive_path.display()
            );
            cmd.arg(ps_command).current_dir(dir);

            let output = cmd
                .output()
                .expect("Failed to execute PowerShell Compress-Archive");
            assert!(
                output.status.success(),
                "PowerShell Compress-Archive failed: {output:?}"
            );
        }

        // On Unix, use system zip command
        #[cfg(not(target_os = "windows"))]
        {
            let mut cmd = std::process::Command::new("zip");
            cmd.arg("-r").arg(&archive_path).current_dir(dir);

            for file in files {
                cmd.arg(file);
            }

            let output = cmd.output().expect("Failed to execute zip command");
            assert!(output.status.success(), "zip command failed: {output:?}");
        }

        archive_path
    }

    #[test]
    #[cfg(feature = "tar")]
    fn test_cli_tar_single_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files
        create_test_file(temp_dir.path(), "test1.txt", b"Hello from TAR test 1");
        create_test_file(temp_dir.path(), "test2.txt", b"Hello from TAR test 2");

        // Create TAR archive
        let tar_path = create_system_tar(temp_dir.path(), "test.tar", &["test1.txt", "test2.txt"]);

        // Test checkle with specific file in archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(format!("{}:test1.txt", tar_path.display()))
            .arg("--algorithm")
            .arg("sha256")
            .arg("--hash")
            .arg("7c59a05c911d7a47fa7db5224040e465c7ef022030ab427d610e4428c6681b63"); // SHA256 of "Hello from TAR test 1"

        cmd.assert().success(); // Verify command succeeds silently
    }

    #[test]
    #[cfg(feature = "zip")]
    fn test_cli_zip_single_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files with more realistic content that will compress well
        let content1 = "This is a test file with enough content to compress well. ".repeat(50);
        let content2 = "Another test file with repeating patterns for compression. ".repeat(50);
        create_test_file(temp_dir.path(), "file1.txt", content1.as_bytes());
        create_test_file(temp_dir.path(), "file2.txt", content2.as_bytes());

        // Create ZIP archive
        let zip_path = create_system_zip(temp_dir.path(), "test.zip", &["file1.txt", "file2.txt"]);

        // Test checkle with specific file in archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(format!("{}:file1.txt", zip_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .arg("--hash")
            .arg("f20f3f33a49604d84d178e775ff4490c"); // MD5 of content1

        cmd.assert().success(); // Verify command succeeds silently
    }

    #[test]
    #[ignore = "Disabled: Tests old behavior where --recursive looked inside archives. Should be rewritten to use explicit 'archive.tar:*' syntax after Phase 2 implementation"]
    #[cfg(feature = "tar")]
    fn test_cli_tar_all_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create multiple test files
        for i in 1..=5 {
            create_test_file(
                temp_dir.path(),
                &format!("file{i}.txt"),
                format!("Content {i}").as_bytes(),
            );
        }

        // Create TAR archive
        let tar_path = create_system_tar(
            temp_dir.path(),
            "multi.tar",
            &[
                "file1.txt",
                "file2.txt",
                "file3.txt",
                "file4.txt",
                "file5.txt",
            ],
        );

        // Test checkle hash with entire archive (recursive)
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(&tar_path)
            .arg("--recursive")
            .arg("--algorithm")
            .arg("sha256");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("file1.txt"))
            .stdout(predicate::str::contains("file2.txt"))
            .stdout(predicate::str::contains("file3.txt"))
            .stdout(predicate::str::contains("file4.txt"))
            .stdout(predicate::str::contains("file5.txt"));
    }

    #[test]
    #[ignore = "Disabled: Tests old behavior where --recursive looked inside archives. Should be rewritten to use explicit 'archive.zip:*' syntax after Phase 2 implementation"]
    #[cfg(feature = "zip")]
    fn test_cli_zip_nested_structure() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create nested directory structure with larger files that compress well
        let root_content = "Root file with substantial content for compression. ".repeat(100);
        let dir1_content = "Directory 1 file with repeating patterns. ".repeat(100);
        let nested_content = "Deeply nested file with compressible data. ".repeat(100);
        let another_content = "Another file with lots of repeated text. ".repeat(100);

        create_test_file(temp_dir.path(), "root.txt", root_content.as_bytes());
        create_test_file(temp_dir.path(), "dir1/file1.txt", dir1_content.as_bytes());
        create_test_file(
            temp_dir.path(),
            "dir1/dir2/nested.txt",
            nested_content.as_bytes(),
        );
        create_test_file(
            temp_dir.path(),
            "dir3/another.txt",
            another_content.as_bytes(),
        );

        // Create ZIP archive
        let zip_path =
            create_system_zip(temp_dir.path(), "nested.zip", &["root.txt", "dir1", "dir3"]);

        // Test checkle with nested archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(&zip_path)
            .arg("--recursive")
            .arg("--format")
            .arg("json");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("root.txt"))
            .stdout(predicate::str::contains("dir1/file1.txt"))
            .stdout(predicate::str::contains("dir1/dir2/nested.txt"))
            .stdout(predicate::str::contains("dir3/another.txt"));
    }

    #[test]
    #[cfg(feature = "tar")]
    fn test_cli_tar_compare_mode() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test file
        let content = b"Content for comparison";
        create_test_file(temp_dir.path(), "compare.txt", content);

        // Create TAR archive
        let tar_path = create_system_tar(temp_dir.path(), "compare.tar", &["compare.txt"]);

        // First, get the hash of the file in the archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(format!("{}:compare.txt", tar_path.display()))
            .arg("--algorithm")
            .arg("sha256")
            .arg("--format")
            .arg("json");

        let output = cmd.output().expect("Failed to execute command");
        assert!(output.status.success());

        // Extract hash from JSON output
        let json_str = String::from_utf8(output.stdout).expect("Failed to convert output to UTF-8");
        let hash_regex =
            regex::Regex::new(r#""hash":\s*"([a-f0-9]+)""#).expect("Failed to compile hash regex");
        let hash = hash_regex
            .captures(&json_str)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str())
            .expect("Failed to extract hash");

        // Now test compare mode (using verify command)
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(format!("{}:compare.txt", tar_path.display()))
            .arg("--hash")
            .arg(hash)
            .arg("--algorithm")
            .arg("sha256");

        cmd.assert().success(); // Verify command succeeds silently
    }

    #[test]
    #[ignore = "Pattern filtering (--include/--exclude) not yet implemented for archive traversal"]
    #[cfg(any(feature = "tar", feature = "zip"))]
    fn test_cli_archive_with_pattern() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create files with different extensions
        create_test_file(temp_dir.path(), "code.rs", b"Rust code");
        create_test_file(temp_dir.path(), "data.txt", b"Text data");
        create_test_file(temp_dir.path(), "script.py", b"Python script");
        create_test_file(temp_dir.path(), "notes.md", b"Markdown notes");

        #[cfg(feature = "tar")]
        {
            let tar_path = create_system_tar(
                temp_dir.path(),
                "mixed.tar",
                &["code.rs", "data.txt", "script.py", "notes.md"],
            );

            // Test with include pattern
            let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
            cmd.arg("hash")
                .arg(&tar_path)
                .arg("--recursive")
                .arg("--include")
                .arg("*.rs")
                .arg("--include")
                .arg("*.py");

            cmd.assert()
                .success()
                .stdout(predicate::str::contains("code.rs"))
                .stdout(predicate::str::contains("script.py"))
                .stdout(
                    predicate::str::is_match("data.txt")
                        .expect("Failed to create predicate")
                        .not(),
                )
                .stdout(
                    predicate::str::is_match("notes.md")
                        .expect("Failed to create predicate")
                        .not(),
                );
        }
    }

    #[test]
    #[ignore = "Disabled: Tests old behavior expecting progress output for files inside archives. Should be rewritten to use 'archive.tar:*' syntax after Phase 2 implementation"]
    #[cfg(feature = "tar")]
    fn test_cli_tar_progress_output() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create larger files to ensure progress reporting
        let large_content = vec![b'A'; 1024 * 1024]; // 1MB
        for i in 1..=10 {
            create_test_file(temp_dir.path(), &format!("large{i}.bin"), &large_content);
        }

        // Create TAR archive
        let files: Vec<String> = (1..=10).map(|i| format!("large{i}.bin")).collect();
        let file_refs: Vec<&str> = files.iter().map(std::string::String::as_str).collect();
        let tar_path = create_system_tar(temp_dir.path(), "large.tar", &file_refs);

        // Test with progress output
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash").arg(&tar_path).arg("--recursive").arg("-v"); // Verbose for progress

        // Progress output should show basic logging and verbose info
        cmd.assert()
            .success()
            .stderr(predicate::str::contains("INFO"))
            .stdout(predicate::str::contains("large1.bin"))
            .stdout(predicate::str::contains("large10.bin"));
    }

    #[test]
    #[cfg(any(feature = "tar", feature = "zip"))]
    #[cfg_attr(
        target_os = "windows",
        ignore = "Stack overflow on Windows with corrupt archives - needs investigation"
    )]
    fn test_cli_archive_error_handling() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Test non-existent archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_dir.path().join("nonexistent.tar"))
            .arg("--hash")
            .arg("dummy_hash_for_test");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Error"));

        // Test non-existent file in archive
        #[cfg(feature = "tar")]
        {
            create_test_file(temp_dir.path(), "exists.txt", b"content");
            let tar_path = create_system_tar(temp_dir.path(), "partial.tar", &["exists.txt"]);

            let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
            cmd.arg("verify")
                .arg(format!("{}:nonexistent.txt", tar_path.display()))
                .arg("--hash")
                .arg("dummy_hash_for_test");

            cmd.assert()
                .failure()
                .stderr(predicate::str::contains("not found"));
        }

        // Test corrupted archive
        let corrupt_path = temp_dir.path().join("corrupt.tar");
        fs::write(&corrupt_path, b"This is not a valid archive")
            .expect("Failed to write corrupt archive");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(&corrupt_path)
            .arg("--hash")
            .arg("dummy_hash_for_test");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Error"));
    }

    #[test]
    #[ignore = "Compressed archive support needs additional implementation for .tar.gz formats"]
    #[cfg(feature = "tar")]
    fn test_cli_compressed_tar_formats() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test file
        create_test_file(
            temp_dir.path(),
            "compress_test.txt",
            b"Compression test content",
        );

        // Test .tar.gz format
        let tar_path = create_system_tar(temp_dir.path(), "test.tar", &["compress_test.txt"]);

        // Compress with gzip
        let gz_path = temp_dir.path().join("test.tar.gz");
        let output = std::process::Command::new("gzip")
            .arg("-c")
            .arg(&tar_path)
            .output()
            .expect("Failed to run gzip");

        fs::write(&gz_path, output.stdout).expect("Failed to write compressed archive");

        // Test checkle with compressed archive (need --recursive to traverse archive contents)
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash").arg(&gz_path).arg("--recursive");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("compress_test.txt"));
    }

    #[test]
    #[ignore = "Batch archive processing not yet integrated with CLI"]
    #[cfg(any(feature = "tar", feature = "zip"))]
    fn test_cli_batch_archive_processing() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create multiple archives
        #[cfg(feature = "tar")]
        {
            for i in 1..=3 {
                create_test_file(
                    temp_dir.path(),
                    &format!("tar{i}.txt"),
                    format!("TAR content {i}").as_bytes(),
                );
                create_system_tar(
                    temp_dir.path(),
                    &format!("archive{i}.tar"),
                    &[&format!("tar{i}.txt")],
                );
            }
        }

        #[cfg(feature = "zip")]
        {
            for i in 1..=3 {
                create_test_file(
                    temp_dir.path(),
                    &format!("zip{i}.txt"),
                    format!("ZIP content {i}").as_bytes(),
                );
                create_system_zip(
                    temp_dir.path(),
                    &format!("archive{i}.zip"),
                    &[&format!("zip{i}.txt")],
                );
            }
        }

        // Test batch processing with glob pattern
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_dir.path().join("archive*"))
            .arg("--format")
            .arg("text")
            .arg("--pretty");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("files"));
    }

    // ============================================================================
    // NEW TESTS: CLI Argument Validation (Required by Three-Test Rule)
    // ============================================================================

    /// Test 1: Verify CLI rejects invalid --algo argument and suggests correct --algorithm
    #[test]
    fn test_cli_invalid_algo_argument_rejection() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        create_test_file(temp_dir.path(), "test.txt", b"test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_dir.path().join("test.txt"))
            .arg("--algo") // Invalid argument
            .arg("md5")
            .arg("--hash")
            .arg("dummy_hash");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains(
                "unexpected argument '--algo' found",
            ))
            .stderr(predicate::str::contains(
                "tip: a similar argument exists: '--algorithm'",
            ));
    }

    /// Test 2: Verify CLI rejects invalid --output-format and suggests --format
    #[test]
    fn test_cli_invalid_output_format_argument_rejection() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        create_test_file(temp_dir.path(), "test.txt", b"test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("hash")
            .arg(temp_dir.path().join("test.txt"))
            .arg("--output-format") // Invalid argument
            .arg("json");

        cmd.assert().failure().stderr(predicate::str::contains(
            "unexpected argument '--output-format' found",
        ));
    }

    /// Test 3: Verify CLI requires --hash argument for verify command
    #[test]
    fn test_cli_verify_requires_hash_argument() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        create_test_file(temp_dir.path(), "test.txt", b"test content");

        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify")
            .arg(temp_dir.path().join("test.txt"))
            .arg("--algorithm")
            .arg("md5");
        // Missing required --hash argument

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains(
                "the following required arguments were not provided",
            ))
            .stderr(predicate::str::contains("--hash <HASH>"));
    }
}
