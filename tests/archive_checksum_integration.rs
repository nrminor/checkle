//! Integration tests for archive checksum verification functionality.
//!
//! These tests verify that the verify-many command can read checksum files
//! from within archives and verify files listed in those checksums.

#[cfg(test)]
mod archive_checksum_tests {
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

    /// Helper to compute MD5 hash of content.
    fn compute_md5(content: &[u8]) -> String {
        use md5::{Digest, Md5};
        let mut hasher = Md5::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
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
    fn test_verify_many_with_checksum_in_tar_archive() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files
        let content1 = b"Hello from file 1";
        let content2 = b"Hello from file 2 with more content";
        let content3 = b"Third file content here";

        create_test_file(temp_dir.path(), "data/file1.txt", content1);
        create_test_file(temp_dir.path(), "data/file2.txt", content2);
        create_test_file(temp_dir.path(), "data/file3.txt", content3);

        // Create checksum file with MD5 hashes
        let checksums_content = format!(
            "{}\tdata/file1.txt\n{}\tdata/file2.txt\n{}\tdata/file3.txt\n",
            compute_md5(content1),
            compute_md5(content2),
            compute_md5(content3)
        );
        create_test_file(
            temp_dir.path(),
            "checksums.md5",
            checksums_content.as_bytes(),
        );

        // Create TAR archive containing both checksum file and data files
        let tar_path =
            create_system_tar(temp_dir.path(), "test_data.tar", &["checksums.md5", "data"]);

        // Test verify-many with checksum file inside archive
        // Note: Files referenced in the checksum must exist on the filesystem
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:checksums.md5", tar_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .current_dir(temp_dir.path()); // Set working directory so relative paths work

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("data/file1.txt"))
            .stdout(predicate::str::contains("data/file2.txt"))
            .stdout(predicate::str::contains("data/file3.txt"));
    }

    #[test]
    #[cfg(feature = "zip")]
    fn test_verify_many_with_checksum_in_zip_archive() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files
        let content1 = b"ZIP test content 1";
        let content2 = b"ZIP test content 2 with more data";

        create_test_file(temp_dir.path(), "files/test1.dat", content1);
        create_test_file(temp_dir.path(), "files/test2.dat", content2);

        // Create checksum file
        let checksums_content = format!(
            "{}\tfiles/test1.dat\n{}\tfiles/test2.dat\n",
            compute_md5(content1),
            compute_md5(content2)
        );
        create_test_file(
            temp_dir.path(),
            "validation/checksums.md5",
            checksums_content.as_bytes(),
        );

        // Create ZIP archive
        let zip_path =
            create_system_zip(temp_dir.path(), "test_data.zip", &["validation", "files"]);

        // Test verify-many with checksum file inside archive
        // Note: Files referenced in the checksum must exist on the filesystem
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:validation/checksums.md5", zip_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .current_dir(temp_dir.path()); // Set working directory so relative paths work

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("files/test1.dat"))
            .stdout(predicate::str::contains("files/test2.dat"));
    }

    #[test]
    #[cfg(feature = "tar")]
    #[cfg_attr(
        target_os = "windows",
        ignore = "Stack overflow on Windows - needs investigation"
    )]
    fn test_verify_many_with_pretty_output_from_archive() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test file
        let content = b"Test content for pretty output";
        create_test_file(temp_dir.path(), "test.txt", content);

        // Create checksum file
        let checksums_content = format!("{}\ttest.txt\n", compute_md5(content));
        create_test_file(
            temp_dir.path(),
            "checksums.md5",
            checksums_content.as_bytes(),
        );

        // Create TAR archive
        let tar_path = create_system_tar(
            temp_dir.path(),
            "pretty_test.tar",
            &["checksums.md5", "test.txt"],
        );

        // Test verify-many with pretty output
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:checksums.md5", tar_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .arg("--pretty")
            .current_dir(temp_dir.path()); // Set working directory so relative paths work

        cmd.assert()
            .success()
            .stderr(predicate::str::contains("Verification Results"))
            .stderr(predicate::str::contains("test.txt"))
            .stderr(predicate::str::contains("PASS"));
    }

    #[test]
    #[cfg(feature = "tar")]
    fn test_verify_many_mixed_archive_and_filesystem() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files - one in archive, one on filesystem
        let archive_content = b"Content in archive";
        let fs_content = b"Content on filesystem";

        create_test_file(temp_dir.path(), "archive_file.txt", archive_content);
        let _fs_file = create_test_file(temp_dir.path(), "filesystem_file.txt", fs_content);

        // Create checksum file that references both
        let checksums_content = format!(
            "{}\tarchive_file.txt\n{}\tfilesystem_file.txt\n",
            compute_md5(archive_content),
            compute_md5(fs_content)
        );
        create_test_file(
            temp_dir.path(),
            "mixed_checksums.md5",
            checksums_content.as_bytes(),
        );

        // Create TAR archive with only the checksum file and one data file
        let tar_path = create_system_tar(
            temp_dir.path(),
            "mixed.tar",
            &["mixed_checksums.md5", "archive_file.txt"],
        );

        // Test verify-many - should find archive_file.txt in archive and filesystem_file.txt on disk
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:mixed_checksums.md5", tar_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .current_dir(temp_dir.path()); // Set working directory so it can find filesystem_file.txt

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("archive_file.txt"))
            .stdout(predicate::str::contains("filesystem_file.txt"));
    }

    #[test]
    #[cfg(feature = "tar")]
    #[cfg_attr(
        target_os = "windows",
        ignore = "Stack overflow on Windows - needs investigation"
    )]
    fn test_verify_many_with_missing_files_in_archive() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create only some of the files referenced in checksums
        let content = b"Only file that exists";
        create_test_file(temp_dir.path(), "exists.txt", content);

        // Create checksum file that references files that don't exist
        let checksums_content = format!(
            "{}\texists.txt\n{}\tmissing1.txt\n{}\tmissing2.txt\n",
            compute_md5(content),
            compute_md5(b"dummy"),
            compute_md5(b"dummy2")
        );
        create_test_file(
            temp_dir.path(),
            "checksums_with_missing.md5",
            checksums_content.as_bytes(),
        );

        // Create TAR archive
        let tar_path = create_system_tar(
            temp_dir.path(),
            "incomplete.tar",
            &["checksums_with_missing.md5", "exists.txt"],
        );

        // Test verify-many - should handle missing files gracefully
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:checksums_with_missing.md5", tar_path.display()))
            .arg("--algorithm")
            .arg("md5")
            .current_dir(temp_dir.path()); // Set working directory so relative paths work

        // The command should succeed but only verify the file that exists
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("exists.txt"));
    }

    #[test]
    #[cfg(any(feature = "tar", feature = "zip"))]
    fn test_verify_many_invalid_archive_path() {
        // Test with non-existent archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg("nonexistent.tar:checksums.md5")
            .arg("--algorithm")
            .arg("md5");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Error"))
            .stderr(predicate::str::contains("does not exist"));
    }

    #[test]
    #[cfg(feature = "tar")]
    fn test_verify_many_nonexistent_checksum_in_archive() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create a simple file
        create_test_file(temp_dir.path(), "dummy.txt", b"dummy content");

        // Create TAR archive without checksums file
        let tar_path = create_system_tar(temp_dir.path(), "no_checksums.tar", &["dummy.txt"]);

        // Test verify-many with non-existent checksum file in archive
        let mut cmd = Command::cargo_bin("checkle").expect("Failed to find checkle binary");
        cmd.arg("verify-many")
            .arg("--checksum-file")
            .arg(format!("{}:missing_checksums.md5", tar_path.display()))
            .arg("--algorithm")
            .arg("md5");

        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("Error"));
    }
}
