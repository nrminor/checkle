//! Debug test to isolate TAR reading issue
//! This will help us understand if checkle's TAR reading is corrupting data

#[cfg(test)]
mod debug_tests {
    use std::fs::File;
    use tar::Builder;
    use tempfile::TempDir;

    // Import checkle's TAR implementation
    use checkle::archive::{ArchiveReader, TarArchive};

    #[test]
    fn test_simple_tar_roundtrip() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_content = b"Hello, world! This is a simple test.";
        let filename = "simple_test.txt";

        // Step 1: Create TAR archive using standard tar crate (like proptest does)
        let archive_path = temp_dir.path().join("simple_test.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        let mut header = tar::Header::new_gnu();
        header.set_path(filename).expect("Failed to set path");
        header.set_size(test_content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        builder
            .append(&header, &test_content[..])
            .expect("Failed to append to TAR");
        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Step 2: Read back using checkle's TAR implementation
        let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR with checkle");

        let (mut entry, metadata) = archive
            .find_entry(filename)
            .expect("Failed to find entry")
            .expect("Entry not found");

        let mut read_content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut read_content)
            .expect("Failed to read entry content");

        // Critical assertions
        assert_eq!(
            test_content.len(),
            read_content.len(),
            "Content lengths must match"
        );
        assert_eq!(
            test_content,
            &read_content[..],
            "Content must match exactly"
        );
        assert_eq!(
            metadata.size,
            test_content.len() as u64,
            "Metadata size must match"
        );
    }

    #[test]
    fn test_binary_tar_roundtrip() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        // Test with binary data that could reveal corruption
        let test_content: Vec<u8> = (0..=255).cycle().take(1024).collect();
        let filename = "binary_test.bin";

        // Step 1: Create TAR archive
        let archive_path = temp_dir.path().join("binary_test.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        let mut header = tar::Header::new_gnu();
        header.set_path(filename).expect("Failed to set path");
        header.set_size(test_content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        builder
            .append(&header, &test_content[..])
            .expect("Failed to append to TAR");
        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Step 2: Read back using checkle
        let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR with checkle");

        let (mut entry, _metadata) = archive
            .find_entry(filename)
            .expect("Failed to find entry")
            .expect("Entry not found");

        let mut read_content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut read_content)
            .expect("Failed to read entry content");

        assert_eq!(
            test_content.len(),
            read_content.len(),
            "Binary content lengths must match"
        );
        assert_eq!(
            test_content, read_content,
            "Binary content must match exactly"
        );
    }

    #[test]
    fn test_proptest_like_content() {
        // Test with the exact kind of content that proptest generates
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Random-like content similar to what proptest generates
        let test_content: Vec<u8> = vec![
            219, 183, 218, 24, 74, 201, 49, 228, 237, 1, 210, 179, 199, 225, 149, 141, 82, 18, 120,
            52, 15, 174, 107, 125, 224, 137, 110, 192, 248, 89, 240, 61, 43, 58, 23, 42, 103, 226,
            25, 174, 198, 0, 206, 194, 74, 24, 61, 68,
        ];
        let filename = "proptest_like.bin";

        // Create TAR
        let archive_path = temp_dir.path().join("proptest_like.tar");
        let file = File::create(&archive_path).expect("Failed to create TAR file");
        let mut builder = Builder::new(file);

        let mut header = tar::Header::new_gnu();
        header.set_path(filename).expect("Failed to set path");
        header.set_size(test_content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        builder
            .append(&header, &test_content[..])
            .expect("Failed to append to TAR");
        builder.finish().expect("Failed to finish TAR");
        drop(builder);

        // Read back
        let mut archive = TarArchive::open(&archive_path).expect("Failed to open TAR with checkle");
        let (mut entry, _metadata) = archive
            .find_entry(filename)
            .expect("Failed to find entry")
            .expect("Entry not found");

        let mut read_content = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut read_content)
            .expect("Failed to read entry content");

        assert_eq!(
            test_content, read_content,
            "Proptest-like content must match exactly"
        );
    }
}
