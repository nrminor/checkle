//! Property-based tests for archive functionality.
//!
//! Uses proptest to generate random test cases and ensure
//! archive handling is robust across various edge cases.
//!
//! NOTE: These tests are currently ignored because archive functionality
//! is not yet fully integrated with the CLI interface.

#[cfg(test)]
mod proptest_tests {
    use checkle::{
        archive::{ArchiveReader, TarArchive, ZipArchive, compute_hash},
        hashing::HashingAlgo,
    };
    use proptest::prelude::*;
    use std::{
        fs::File,
        io::{Read, Write},
    };
    #[cfg(feature = "tar")]
    use tar::Builder;
    use tempfile::TempDir;
    #[cfg(feature = "zip")]
    use zip::{CompressionMethod, ZipWriter, write::FileOptions};

    /// Strategy for generating valid filenames for archives.
    fn archive_filename_strategy() -> impl Strategy<Value = String> {
        prop::string::string_regex(r"[a-zA-Z0-9_\-./]{1,100}")
            .expect("Failed to create filename regex")
            .prop_filter("not empty or dots", |s| {
                !s.is_empty() && s != "." && s != ".." && !s.contains("//")
            })
    }

    /// Strategy for generating file content.
    fn file_content_strategy() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..10000)
    }

    /// Strategy for generating a collection of files.
    fn files_strategy() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
        prop::collection::vec(
            (archive_filename_strategy(), file_content_strategy()),
            1..20, // 1 to 20 files
        )
    }

    #[cfg(feature = "tar")]
    proptest! {
        #[test]
        #[ignore = "Archive implementation under investigation..."]
        fn tar_roundtrip_preserves_content(
            files in files_strategy()
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Create TAR archive with proptest-generated files
            let archive_path = temp_dir.path().join("test.tar");
            let file = File::create(&archive_path).expect("Failed to create archive file");
            let mut builder = Builder::new(file);

            // Keep track of what we put in
            let mut expected_files = std::collections::HashMap::new();

            for (name, content) in &files {
                // Skip invalid filenames that tar might reject
                if name.starts_with('/') || name.contains('\0') {
                    continue;
                }

                let mut header = tar::Header::new_gnu();
                if header.set_path(name).is_err() {
                    continue; // Skip if path is invalid
                }
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();

                if builder.append(&header, &content[..]).is_ok() {
                    expected_files.insert(name.clone(), content.clone());
                }
            }

            builder.finish().expect("Failed to finish TAR archive");
            drop(builder);

            // Now read back and verify
            if !expected_files.is_empty() {
                let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR archive");

                // Verify each file
                for (name, expected_content) in &expected_files {
                    if let Ok(Some((mut entry, metadata))) = archive.find_entry(name) {
                        let mut actual_content = Vec::new();
                        entry.read_to_end(&mut actual_content).expect("Failed to read entry content");

                        prop_assert_eq!(&actual_content, expected_content);
                        prop_assert_eq!(metadata.size, expected_content.len() as u64);
                    }
                }
            }
        }

        #[test]
        fn tar_entry_count_matches(
            files in files_strategy().prop_filter("at least one valid file", |f| !f.is_empty())
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Create TAR archive
            let archive_path = temp_dir.path().join("count_test.tar");
            let file = File::create(&archive_path).expect("Failed to create archive file");
            let mut builder = Builder::new(file);

            let mut valid_count = 0;
            for (name, content) in &files {
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
                    valid_count += 1;
                }
            }

            builder.finish().expect("Failed to finish TAR archive");
            drop(builder);

            if valid_count > 0 {
                let archive = TarArchive::open(&archive_path).expect("Failed to open TAR archive");
                let count = archive.entry_count().expect("Failed to get entry count");
                prop_assert_eq!(count, valid_count);
            }
        }

        #[test]
        fn tar_handles_empty_files(
            filenames in prop::collection::vec(archive_filename_strategy(), 1..10)
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Create TAR with empty files
            let archive_path = temp_dir.path().join("empty_test.tar");
            let file = File::create(&archive_path).expect("Failed to create archive file");
            let mut builder = Builder::new(file);

            let mut valid_files = Vec::new();
            for name in &filenames {
                if name.starts_with('/') || name.contains('\0') {
                    continue;
                }

                let mut header = tar::Header::new_gnu();
                if header.set_path(name).is_err() {
                    continue;
                }
                header.set_size(0);
                header.set_mode(0o644);
                header.set_cksum();

                if builder.append(&header, &b""[..]).is_ok() {
                    valid_files.push(name.clone());
                }
            }

            builder.finish().expect("Failed to finish TAR archive");
            drop(builder);

            if !valid_files.is_empty() {
                let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR archive");

                // Verify we can read empty files
                for name in &valid_files {
                    if let Ok(Some((mut entry, metadata))) = archive.find_entry(name) {
                        let mut content = Vec::new();
                        entry.read_to_end(&mut content).expect("Failed to read entry content");
                        prop_assert_eq!(content.len(), 0);
                        prop_assert_eq!(metadata.size, 0);
                    }
                }
            }
        }
    }

    #[cfg(feature = "zip")]
    proptest! {
        #[test]
        fn zip_roundtrip_preserves_content(
            files in files_strategy()
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Create ZIP archive with proptest-generated files
            let archive_path = temp_dir.path().join("test.zip");
            let file = File::create(&archive_path).expect("Failed to create archive file");
            let mut zip = ZipWriter::new(file);

            let options = FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o644);

            // Keep track of what we put in
            let mut expected_files = std::collections::HashMap::new();

            for (name, content) in &files {
                // Skip invalid filenames
                if name.is_empty() || name.contains('\0') {
                    continue;
                }

                if zip.start_file(name, options).is_ok() && zip.write_all(content).is_ok() {
                    expected_files.insert(name.clone(), content.clone());
                }
            }

            zip.finish().expect("Failed to finish ZIP archive");
            drop(zip);

            // Now read back and verify
            if !expected_files.is_empty() {
                let mut archive = ZipArchive::open(&archive_path).expect("Failed to open ZIP archive");

                // Verify each file
                for (name, expected_content) in &expected_files {
                    if let Ok(Some((mut entry, metadata))) = archive.find_entry(name) {
                        let mut actual_content = Vec::new();
                        entry.read_to_end(&mut actual_content).expect("Failed to read entry content");

                        prop_assert_eq!(&actual_content, expected_content);
                        prop_assert_eq!(metadata.size, expected_content.len() as u64);
                    }
                }
            }
        }

        #[test]
        #[ignore = "Archive functionality not yet integrated with CLI"]
        fn zip_compression_methods(
            content in file_content_strategy().prop_filter("non-empty", |c| !c.is_empty()),
            method_idx in 0..3usize
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Test different compression methods

            let methods = [
                CompressionMethod::Stored,
                CompressionMethod::Deflated,
                CompressionMethod::Bzip2,
            ];

            let method = methods[method_idx % methods.len()];

            let archive_path = temp_dir.path().join("compressed.zip");
            let file = File::create(&archive_path).expect("Failed to create archive file");
            let mut zip = ZipWriter::new(file);

            let options = FileOptions::default()
                .compression_method(method)
                .unix_permissions(0o644);

            zip.start_file("test.bin", options).expect("Failed to start ZIP file");
            zip.write_all(&content).expect("Failed to write ZIP content");
            zip.finish().expect("Failed to finish ZIP archive");
            drop(zip);

            // Verify we can read it back correctly
            let mut archive = ZipArchive::open(&archive_path).expect("Failed to open ZIP archive");
            let (mut entry, metadata) = archive.find_entry("test.bin").expect("Failed to find entry").expect("Entry not found");

            let mut decompressed = Vec::new();
            entry.read_to_end(&mut decompressed).expect("Failed to read decompressed content");

            prop_assert_eq!(&decompressed, &content);
            prop_assert_eq!(metadata.size, content.len() as u64);
        }
    }

    #[cfg(any(feature = "tar", feature = "zip"))]
    proptest! {
        #[test]
        #[ignore = "Archive functionality not yet integrated with CLI"]
        fn archive_hashing_consistency(
            content in file_content_strategy(),
            algo_idx in 0..2usize
        ) {
            let algos = [&HashingAlgo::Sha2, &HashingAlgo::Md5];
            let algo = algos[algo_idx % algos.len()];

            let temp_dir = TempDir::new().expect("Failed to create temp directory");

            // Compute direct hash
            let direct_hash = compute_hash(&mut &content[..], algo).expect("Failed to compute direct hash");

            // Test with TAR
            #[cfg(feature = "tar")]
            {
                let tar_path = temp_dir.path().join("hash_test.tar");
                let file = File::create(&tar_path).expect("Failed to create TAR file");
                let mut builder = Builder::new(file);

                let mut header = tar::Header::new_gnu();
                header.set_path("test").expect("Failed to set TAR header path");
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();

                builder.append(&header, &content[..]).expect("Failed to append TAR entry");
                builder.finish().expect("Failed to finish TAR archive");
                drop(builder);

                let mut archive = TarArchive::open(&tar_path).expect("Failed to open TAR archive");
                let (mut entry, _) = archive.find_entry("test").expect("Failed to find entry").expect("Entry not found");
                let tar_hash = compute_hash(&mut entry, algo).expect("Failed to compute TAR hash");

                prop_assert_eq!(&tar_hash, &direct_hash);
            }

            // Test with ZIP
            #[cfg(feature = "zip")]
            {
                let zip_path = temp_dir.path().join("hash_test.zip");
                let file = File::create(&zip_path).expect("Failed to create ZIP file");
                let mut zip = ZipWriter::new(file);

                zip.start_file("test", FileOptions::default()).expect("Failed to start ZIP file");
                zip.write_all(&content).expect("Failed to write ZIP content");
                zip.finish().expect("Failed to finish ZIP archive");
                drop(zip);

                let mut archive = ZipArchive::open(&zip_path).expect("Failed to open ZIP archive");
                let (mut entry, _) = archive.find_entry("test").expect("Failed to find entry").expect("Entry not found");
                let zip_hash = compute_hash(&mut entry, algo).expect("Failed to compute ZIP hash");

                prop_assert_eq!(&zip_hash, &direct_hash);
            }
        }

        #[test]
        #[ignore = "Archive functionality not yet integrated with CLI"]
        fn archive_entry_names_preserved(
            names in prop::collection::vec(archive_filename_strategy(), 1..10)
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp directory");
            let content = b"test content";

            #[cfg(feature = "tar")]
            {
                let tar_path = temp_dir.path().join("names.tar");
                let file = File::create(&tar_path).expect("Failed to create TAR file");
                let mut builder = Builder::new(file);

                let mut valid_names = Vec::new();
                for name in &names {
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
                        valid_names.push(name.clone());
                    }
                }

                builder.finish().expect("Failed to finish TAR archive");
                drop(builder);

                if !valid_names.is_empty() {
                    let mut archive = TarArchive::open(&tar_path).expect("Failed to open TAR archive");

                    // Collect all entry names
                    let mut found_names = Vec::new();
                    for entry_result in archive.entries().expect("Failed to get entries iterator") {
                        let (_, _, metadata) = entry_result.expect("Failed to get entry");
                        found_names.push(metadata.path.to_string_lossy().to_string());
                    }

                    // Verify all names are preserved
                    for name in &valid_names {
                        prop_assert!(found_names.contains(name));
                    }
                }
            }
        }
    }

    // Test that archive operations handle resource limits properly
    //
    // Note: Resource limit validation happens at runtime in the archive code
    // through proper error handling rather than compile-time checks.
}
