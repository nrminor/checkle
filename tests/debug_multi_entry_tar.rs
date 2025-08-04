//! Debug test for multi-entry TAR archives (like proptest creates)

#[cfg(test)]
mod debug_multi_entry_tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Read;
    use tar::Builder;
    use tempfile::TempDir;

    // Import checkle's TAR implementation
    use checkle::archive::{ArchiveReader, TarArchive};

    #[test]
    fn test_multi_entry_tar_roundtrip() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create multiple files like proptest does
        let test_files = vec![
            ("file1.txt", b"Content for file 1".to_vec()),
            ("file2.bin", vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
            ("path/nested.txt", b"Nested file content".to_vec()),
            ("special-chars.txt", vec![255, 0, 128, 64, 32, 16]),
        ];

        // Step 1: Create TAR archive with multiple files
        let archive_path = temp_dir.path().join("multi_test.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        let mut expected_files = HashMap::new();

        for (name, content) in &test_files {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).expect("Failed to set path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder
                .append(&header, &content[..])
                .expect("Failed to append to TAR");
            expected_files.insert((*name).to_string(), content.clone());
        }

        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Step 2: Read back each file using checkle
        let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR with checkle");

        for (name, expected_content) in &expected_files {
            let (mut entry, metadata) = archive
                .find_entry(name)
                .expect("Failed to find entry")
                .expect("Entry not found");

            let mut read_content = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut read_content)
                .expect("Failed to read entry content");

            assert_eq!(
                expected_content.len(),
                read_content.len(),
                "Length mismatch for file '{name}'"
            );
            assert_eq!(
                expected_content, &read_content,
                "Content mismatch for file '{name}'"
            );
            assert_eq!(
                metadata.size,
                expected_content.len() as u64,
                "Metadata size mismatch for file '{name}'"
            );
        }
    }

    #[test]
    fn test_proptest_exact_scenario() {
        // Try to replicate the exact proptest scenario more closely
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Use the exact content from the failing proptest (first few bytes)
        let test_files = vec![
            (
                "test_file_1",
                vec![219, 183, 218, 24, 74, 201, 49, 228, 237, 1],
            ),
            (
                "another_file",
                vec![210, 179, 199, 225, 149, 141, 82, 18, 120, 52],
            ),
        ];

        // Create TAR exactly like proptest does
        let archive_path = temp_dir.path().join("proptest_replica.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        let mut expected_files = HashMap::new();

        for (name, content) in &test_files {
            // Skip invalid filenames (like proptest does)
            if name.starts_with('/') || name.contains('\0') {
                continue;
            }

            let mut header = tar::Header::new_gnu();
            if header.set_path(name).is_err() {
                continue;
            }
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            if builder.append(&header, &content[..]).is_ok() {
                expected_files.insert((*name).to_string(), content.clone());
            }
        }

        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Read back exactly like proptest does
        if !expected_files.is_empty() {
            let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR");

            for (name, expected_content) in &expected_files {
                if let Ok(Some((mut entry, metadata))) = archive.find_entry(name) {
                    let mut actual_content = Vec::new();
                    entry
                        .read_to_end(&mut actual_content)
                        .expect("Failed to read entry content");

                    // This is the exact assertion from proptest
                    assert_eq!(
                        &actual_content, expected_content,
                        "Proptest-style assertion failed for '{name}'"
                    );
                    assert_eq!(metadata.size, expected_content.len() as u64);
                }
            }
        }
    }
}
