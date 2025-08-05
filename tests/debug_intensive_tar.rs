//! Intensive TAR testing to try to reproduce the proptest failure

#[cfg(test)]
mod debug_intensive_tests {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Read;
    use tar::Builder;
    use tempfile::TempDir;

    use checkle::archive::{ArchiveReader, TarArchive};

    #[test]
    fn test_many_random_files() {
        // Generate lots of files with various content to stress test
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        let mut test_files = Vec::new();
        let mut rng_state = 12345u64; // Simple LCG for reproducible "random" data

        // Generate 50 files with various patterns
        for i in 0..50 {
            let filename = format!("file_{i}.bin");

            // Generate pseudo-random content of varying lengths
            let content_len = (rng_state % 200) + 1; // 1-200 bytes
            let mut content = Vec::new();

            for _ in 0..content_len {
                rng_state = rng_state.wrapping_mul(1_103_515_245).wrapping_add(12345);
                #[allow(clippy::cast_possible_truncation)]
                {
                    content.push((rng_state >> 16) as u8);
                }
            }

            test_files.push((filename, content));
        }

        // Create TAR
        let archive_path = temp_dir.path().join("intensive_test.tar");
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
            expected_files.insert(name.clone(), content.clone());
        }

        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Read back and verify - multiple times to check for state issues
        for _round in 0..3 {
            let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR");

            for (name, expected_content) in &expected_files {
                let (mut entry, _metadata) = archive
                    .find_entry(name)
                    .expect("Failed to find entry")
                    .expect("Entry not found");

                let mut read_content = Vec::new();
                entry
                    .read_to_end(&mut read_content)
                    .expect("Failed to read entry content");

                if expected_content != &read_content {
                    if expected_content.len() == read_content.len() {
                        // Same length but different content - find first difference
                    }
                    panic!("Content mismatch in intensive test");
                }
            }
        }
    }

    #[test]
    fn test_archive_reuse_same_instance() {
        // Test if there's an issue with reusing the same archive instance
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        let test_files = vec![
            ("first.txt", b"First file content".to_vec()),
            ("second.txt", b"Second file content".to_vec()),
            ("third.txt", b"Third file content".to_vec()),
        ];

        // Create TAR
        let archive_path = temp_dir.path().join("reuse_test.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        for (name, content) in &test_files {
            let mut header = tar::Header::new_gnu();
            header.set_path(name).expect("Failed to set path");
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder
                .append(&header, &content[..])
                .expect("Failed to append to TAR");
        }

        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Use the SAME archive instance to read multiple files
        let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR");

        // Read files in different orders to test for state issues
        let read_orders = [
            vec!["first.txt", "second.txt", "third.txt"],
            vec!["third.txt", "first.txt", "second.txt"],
            vec!["second.txt", "third.txt", "first.txt"],
        ];

        for (round, order) in read_orders.iter().enumerate() {
            for filename in order {
                let expected_content = test_files
                    .iter()
                    .find(|(name, _)| name == filename)
                    .map(|(_, content)| content)
                    .expect("Test file not found");

                let (mut entry, _metadata) = archive
                    .find_entry(filename)
                    .expect("Failed to find entry")
                    .expect("Entry not found");

                let mut read_content = Vec::new();
                entry
                    .read_to_end(&mut read_content)
                    .expect("Failed to read entry content");

                assert_eq!(
                    expected_content,
                    &read_content,
                    "Content mismatch for '{filename}' in round {}",
                    round + 1
                );
            }
        }
    }
}
