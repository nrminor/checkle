use ignore::WalkBuilder;
use log::{debug, warn};
use std::{
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::{
    cli::Recursive,
    constants::{MAX_CHECKSUM_FILE_LINES, MAX_FILES_IN_BATCH, MAX_RECURSION_DEPTH},
    prelude::CheckleError,
};

pub struct FilesToCheck(Vec<FileHashPair>);

impl FilesToCheck {
    /// Creates a new empty `FilesToCheck` collection.
    ///
    /// # Panics
    ///
    /// Panics if the postcondition check fails (the collection is not empty).
    #[must_use]
    pub fn new() -> Self {
        let files_to_check = Self(Vec::new());

        // Postcondition assertion
        assert!(
            files_to_check.0.is_empty(),
            "New FilesToCheck must be empty"
        );

        files_to_check
    }

    /// Creates a `FilesToCheck` collection from a vector of file-hash pairs.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The number of pairs exceeds `MAX_FILES_IN_BATCH` (10,000)
    /// - The postcondition check fails (length doesn't match input)
    #[must_use]
    pub fn from_vec(pairs: Vec<FileHashPair>) -> Self {
        // Precondition assertions
        assert!(
            pairs.len() <= MAX_FILES_IN_BATCH,
            "File batch size exceeds maximum: {} > {}",
            pairs.len(),
            MAX_FILES_IN_BATCH
        );

        let pairs_len = pairs.len();
        let files_to_check = Self(pairs);

        // Postcondition assertion
        assert_eq!(files_to_check.0.len(), pairs_len, "Length must match input");

        files_to_check
    }

    /// Converts the collection into a vector of file-hash pairs.
    ///
    /// # Panics
    ///
    /// Panics if the resulting vector size exceeds `MAX_FILES_IN_BATCH` (10,000).
    #[must_use]
    pub fn to_vec(self) -> Vec<FileHashPair> {
        let vec = self.0;

        // Postcondition assertion
        assert!(
            vec.len() <= MAX_FILES_IN_BATCH,
            "Vector size must not exceed maximum batch size: {} > {}",
            vec.len(),
            MAX_FILES_IN_BATCH
        );

        vec
    }

    /// Adds a file-hash pair to the collection.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Adding the item would exceed `MAX_FILES_IN_BATCH` (10,000)
    /// - The file in the pair doesn't exist
    /// - The postcondition check fails (length doesn't increase by 1)
    pub fn push(&mut self, item: FileHashPair) {
        // Precondition assertions
        assert!(
            self.0.len() < MAX_FILES_IN_BATCH,
            "Cannot push: would exceed maximum batch size"
        );
        assert!(
            item.file().exists(),
            "File must exist: {}",
            item.file().display()
        );

        let old_len = self.0.len();
        self.0.push(item);

        // Postcondition assertion
        assert_eq!(self.0.len(), old_len + 1, "Length must increase by 1");
    }
}

impl Default for FilesToCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct FileHashPair {
    file: PathBuf,
    hash: String,
}

impl FileHashPair {
    /// Creates a new file-hash pair.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The file doesn't exist
    /// - The hash is empty
    /// - The hash contains non-hexadecimal characters
    /// - Any postcondition check fails
    #[must_use]
    pub fn new(file: PathBuf, hash: String) -> Self {
        // Precondition assertions
        assert!(file.exists(), "File must exist: {}", file.display());
        assert!(!hash.is_empty(), "Hash must not be empty");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash must contain only hexadecimal characters"
        );

        let pair = Self { file, hash };

        // Postcondition assertions
        assert!(pair.file().exists(), "File path must exist");
        assert!(!pair.hash().is_empty(), "Hash must not be empty");

        pair
    }

    /// Returns a reference to the file path.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Returns a reference to the hash string.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The hash is empty
    /// - The hash contains non-hexadecimal characters
    #[must_use]
    pub fn hash(&self) -> &str {
        let hash = &self.hash;

        // Postcondition assertions
        assert!(!hash.is_empty(), "Hash must not be empty");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash must contain only hexadecimal characters"
        );

        hash
    }

    /// Returns a mutable reference to the file path.
    #[must_use]
    pub fn file_mut(&mut self) -> &mut Path {
        &mut self.file
    }
    /// Returns a mutable reference to the hash string.
    pub fn hash_mut(&mut self) -> &mut str {
        &mut self.hash
    }
    /// Consumes the pair and returns the owned file path and hash.
    #[must_use]
    pub fn file_hash_owned(self) -> (PathBuf, String) {
        (self.file, self.hash)
    }
}

impl FilesToCheck {
    /// Creates a `FilesToCheck` collection by parsing a checksum file.
    ///
    /// The checksum file should be tab-delimited with two columns:
    /// - First column: hash value
    /// - Second column: file path
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The checksum file cannot be opened
    /// - The file format is invalid (not tab-delimited, wrong number of fields)
    /// - I/O errors occur while reading
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The checksum file doesn't exist
    /// - The checksum file is not a regular file
    /// - The number of lines exceeds `MAX_CHECKSUM_FILE_LINES` (100,000)
    /// - Any postcondition check fails
    pub fn new_from_txt(checksum_file: &Path) -> Result<FilesToCheck, CheckleError> {
        // Precondition assertions
        assert!(
            checksum_file.exists(),
            "Checksum file must exist: {}",
            checksum_file.display()
        );
        assert!(
            checksum_file.is_file(),
            "Checksum file must be a file: {}",
            checksum_file.display()
        );

        let Ok(file_handle) = File::open(checksum_file) else {
            return Err(CheckleError::InaccessibleFile(checksum_file.to_path_buf()));
        };
        let buffer = BufReader::new(file_handle);

        let mut files_to_check = FilesToCheck::new();
        let mut line_count = 0;

        for line in buffer.lines() {
            line_count += 1;
            assert!(
                line_count <= MAX_CHECKSUM_FILE_LINES,
                "Checksum file exceeds maximum line count: {line_count} > {MAX_CHECKSUM_FILE_LINES}"
            );
            let Ok(line) = line else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };

            // Use stack array instead of heap allocation for exactly 2 expected fields
            let mut fields = line.split('\t');
            let Some(hash) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };
            let Some(file_str) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };
            // Ensure no extra fields remain
            if fields.next().is_some() {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            }
            let file_path = PathBuf::from(file_str);
            if !file_path.exists() {
                warn!(
                    "A file listed in the checksum file, {file_str}, does not exist and will be skipped"
                );
                continue;
            }

            let wrapper = FileHashPair::new(file_path, hash.to_string());

            files_to_check.push(wrapper);
        }

        // Postcondition assertions
        // Note: It's valid to have lines that don't result in files being added
        // (e.g., when files don't exist and we log warnings)
        assert!(
            files_to_check.0.len() <= MAX_FILES_IN_BATCH,
            "Result must not exceed maximum batch size"
        );

        Ok(files_to_check)
    }
}

/// Collects file paths based on the input path with optional recursive traversal.
///
/// If the input is a wildcard ("*", "./*", "./", "."), returns files based on recursive flag.
/// If recursive is true and input is a directory, uses ignore crate for efficient traversal.
/// Otherwise, returns the single input file.
///
/// # Errors
///
/// Returns an error if:
/// - Unable to get the current directory (for wildcards)
/// - Unable to read the directory contents
/// - Walk builder encounters an error during traversal
///
/// # Panics
///
/// Panics if:
/// - The input path doesn't exist and isn't a recognized wildcard
/// - A single file input is not a regular file
/// - The number of collected files exceeds `MAX_FILES_IN_BATCH` (10,000)
/// - Any postcondition check fails
pub fn collect_files(input: &Path, recursive: Recursive) -> Result<Vec<PathBuf>, CheckleError> {
    // Precondition assertions
    assert!(
        input.exists()
            || input == Path::new("*")
            || input == Path::new("./*")
            || input == Path::new("./")
            || input == Path::new("."),
        "Input path must exist or be a wildcard: {}",
        input.display()
    );

    // Handle wildcards or directory inputs
    if input == PathBuf::from("*")
        || input == PathBuf::from("./*")
        || input == PathBuf::from("./")
        || input == PathBuf::from(".")
        || input.is_dir()
    {
        let target_dir = if input.is_dir() {
            input.to_path_buf()
        } else {
            env::current_dir().map_err(|source| CheckleError::CurrentDirectoryError { source })?
        };

        if recursive {
            collect_files_recursive(&target_dir)
        } else {
            collect_files_non_recursive(&target_dir)
        }
    } else {
        // Single file case
        assert!(
            input.is_file(),
            "Single input must be a file: {}",
            input.display()
        );

        let wrapped_file = vec![input.to_path_buf()];

        // Postcondition assertion
        assert_eq!(
            wrapped_file.len(),
            1,
            "Single file result must have exactly one element"
        );

        debug!("Preparing to hash {} file(s)...", wrapped_file.len());
        Ok(wrapped_file)
    }
}

/// Collects files from a directory non-recursively.
///
/// # Errors
///
/// Returns an error if unable to read the directory.
///
/// # Panics
///
/// Panics if:
/// - The directory doesn't exist
/// - The file count exceeds `MAX_FILES_IN_BATCH`
fn collect_files_non_recursive(dir: &Path) -> Result<Vec<PathBuf>, CheckleError> {
    // Precondition assertion
    assert!(dir.is_dir(), "Path must be a directory: {}", dir.display());

    let entries = fs::read_dir(dir).map_err(|source| CheckleError::DirectoryReadError {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut file_paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            file_paths.push(path);
        }
    }

    // Postcondition assertion
    assert!(
        file_paths.len() <= MAX_FILES_IN_BATCH,
        "File count exceeds maximum batch size: {} > {}",
        file_paths.len(),
        MAX_FILES_IN_BATCH
    );

    debug!("Preparing to hash {} files...", file_paths.len());
    Ok(file_paths)
}

/// Collects files from a directory recursively using the ignore crate.
///
/// # Errors
///
/// Returns an error if:
/// - Walk builder encounters an error
/// - File count exceeds `MAX_FILES_IN_BATCH`
///
/// # Panics
///
/// Panics if:
/// - The directory doesn't exist
/// - Recursion depth exceeds `MAX_RECURSION_DEPTH`
fn collect_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, CheckleError> {
    // Precondition assertion
    assert!(dir.is_dir(), "Path must be a directory: {}", dir.display());

    let mut file_paths = Vec::new();
    let walker = WalkBuilder::new(dir)
        .hidden(true) // Skip hidden files by default (but .gitignore takes precedence)
        .git_ignore(true) // Respect .gitignore files
        .git_global(false) // Don't use global gitignore
        .git_exclude(false) // Don't use .git/info/exclude
        .max_depth(Some(MAX_RECURSION_DEPTH))
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| CheckleError::DirectoryReadError {
            path: dir.to_path_buf(),
            source: std::io::Error::other(e),
        })?;

        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            if let Some(path) = entry.path().to_owned().into() {
                file_paths.push(path);

                // Check batch size limit during collection
                if file_paths.len() > MAX_FILES_IN_BATCH {
                    warn!(
                        "File collection stopped: exceeded maximum batch size of {MAX_FILES_IN_BATCH} files"
                    );
                    break;
                }
            }
        }
    }

    // Postcondition assertion
    assert!(
        file_paths.len() <= MAX_FILES_IN_BATCH,
        "File count must not exceed maximum batch size"
    );

    debug!(
        "Recursively collected {} files from {}",
        file_paths.len(),
        dir.display()
    );
    Ok(file_paths)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::format_push_string,
        clippy::items_after_statements,
        clippy::format_collect
    )]
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, FileFailurePersistence};
    use std::fs;
    use tempfile::{NamedTempFile, TempDir};

    // Test 1: Normal operation - create new FilesToCheck
    #[test]
    fn test_files_to_check_new() {
        let files_to_check = FilesToCheck::new();
        assert_eq!(
            files_to_check.0.len(),
            0,
            "New FilesToCheck should be empty"
        );

        let vec = files_to_check.to_vec();
        assert!(vec.is_empty(), "Converted vector should be empty");
    }

    // Test 2: Normal operation - FileHashPair creation and access
    #[test]
    fn test_file_hash_pair_normal_operation() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let hash = "abcdef1234567890abcdef1234567890"; // 32 char hex

        let pair = FileHashPair::new(temp_file.path().to_path_buf(), hash.to_string());

        assert_eq!(pair.file(), temp_file.path(), "File path should match");
        assert_eq!(pair.hash(), hash, "Hash should match");

        let (file_owned, hash_owned) = pair.file_hash_owned();
        assert_eq!(file_owned, temp_file.path(), "Owned file path should match");
        assert_eq!(hash_owned, hash, "Owned hash should match");
    }

    // Test 3: Normal operation - FilesToCheck from vector
    #[test]
    fn test_files_to_check_from_vec() {
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

        let pair1 = FileHashPair::new(
            temp_file1.path().to_path_buf(),
            "abcdef1234567890".to_string(),
        );
        let pair2 = FileHashPair::new(
            temp_file2.path().to_path_buf(),
            "123456789abcdef0".to_string(),
        );

        let pair_vec = vec![pair1, pair2];
        let files_to_check = FilesToCheck::from_vec(pair_vec.clone());

        assert_eq!(files_to_check.0.len(), 2, "Should contain 2 pairs");

        let retrieved_vec = files_to_check.to_vec();
        assert_eq!(
            retrieved_vec.len(),
            2,
            "Retrieved vector should have 2 elements"
        );
    }

    // Test 4: Normal operation - pushing to FilesToCheck
    #[test]
    fn test_files_to_check_push() {
        let mut files_to_check = FilesToCheck::new();
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let pair = FileHashPair::new(
            temp_file.path().to_path_buf(),
            "1234567890abcdef".to_string(),
        );

        files_to_check.push(pair);
        assert_eq!(
            files_to_check.0.len(),
            1,
            "Should contain 1 pair after push"
        );

        let vec = files_to_check.to_vec();
        assert_eq!(vec.len(), 1, "Converted vector should have 1 element");
    }

    // Test 5: Normal operation - parsing checksum file
    #[test]
    fn test_parse_checksum_file_normal() {
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

        // Create a checksum file
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        let checksum_content = format!(
            "d41d8cd98f00b204e9800998ecf8427e\t{}\na1b2c3d4e5f67890abcdef1234567890\t{}",
            temp_file1.path().display(),
            temp_file2.path().display()
        );
        fs::write(checksum_file.path(), checksum_content).expect("Failed to write checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(result.is_ok(), "Parsing valid checksum file should succeed");

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert_eq!(vec.len(), 2, "Should parse 2 file-hash pairs");
    }

    // Test 6: Normal operation - collect files wildcard
    #[test]
    fn test_collect_files_wildcard() {
        // Change to a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_dir.path()).expect("Failed to change dir");

        // Create some test files
        let _file1 = NamedTempFile::new_in(temp_dir.path()).expect("Failed to create file");
        let _file2 = NamedTempFile::new_in(temp_dir.path()).expect("Failed to create file");

        let result = collect_files(Path::new("*"), false);

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        assert!(
            result.is_ok(),
            "Collecting files with wildcard should succeed"
        );
        let files = result.unwrap();
        assert!(!files.is_empty(), "Should find at least some files");
    }

    // Test 7: Normal operation - collect single file
    #[test]
    fn test_collect_files_single() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let result = collect_files(temp_file.path(), false);
        assert!(result.is_ok(), "Collecting single file should succeed");

        let files = result.unwrap();
        assert_eq!(files.len(), 1, "Should return exactly one file");
        assert_eq!(
            files[0],
            temp_file.path(),
            "Returned file should match input"
        );
    }

    // Test 8: Edge case - empty checksum file
    #[test]
    fn test_parse_empty_checksum_file() {
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        // Leave the file empty

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(result.is_ok(), "Parsing empty checksum file should succeed");

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert!(
            vec.is_empty(),
            "Empty checksum file should result in empty list"
        );
    }

    // Test 9: Edge case - checksum file with non-existent files
    #[test]
    fn test_parse_checksum_file_missing_files() {
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        let checksum_content = "abcdef1234567890abcdef1234567890\t/nonexistent/file1.txt\n1234567890abcdef1234567890abcdef\t/nonexistent/file2.txt";
        fs::write(checksum_file.path(), checksum_content).expect("Failed to write checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(
            result.is_ok(),
            "Parsing checksum file with missing files should succeed"
        );

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert!(vec.is_empty(), "Should skip non-existent files");
    }

    // Test 10: Error path - invalid checksum file format
    #[test]
    fn test_parse_invalid_checksum_file() {
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        let invalid_content = "hash1 file1.txt\nhash2\t\t\tfile2.txt"; // Wrong delimiter and too many fields
        fs::write(checksum_file.path(), invalid_content)
            .expect("Failed to write invalid checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(result.is_err(), "Parsing invalid checksum file should fail");

        if let Err(CheckleError::InvalidChecksumFile(path)) = result {
            assert_eq!(path, checksum_file.path());
        } else {
            panic!("Expected InvalidChecksumFile error");
        }
    }

    // Test 15: Edge case - maximum batch size
    #[test]
    fn test_max_batch_size_limit() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let mut files_to_check = FilesToCheck::new();

        // Add files up to the limit (we'll add just a few for testing, not the full 10k)
        for i in 0..10 {
            let pair = FileHashPair::new(
                temp_file.path().to_path_buf(),
                format!("{:032x}", i), // Generate valid hex hash
            );
            files_to_check.push(pair);
        }

        assert_eq!(files_to_check.0.len(), 10, "Should contain 10 pairs");
    }

    // Test 16: Edge case - very large checksum file (simulated)
    #[test]
    fn test_large_checksum_file_simulation() {
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        let temp_files: Vec<_> = (0..20)
            .map(|_| NamedTempFile::new().expect("Failed to create temp file"))
            .collect();

        // Create checksum file with many entries
        let mut checksum_content = String::new();
        for (i, temp_file) in temp_files.iter().enumerate() {
            checksum_content.push_str(&format!("{:032x}\t{}\n", i, temp_file.path().display()));
        }
        fs::write(checksum_file.path(), checksum_content)
            .expect("Failed to write large checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(
            result.is_ok(),
            "Large checksum file should parse successfully"
        );

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert_eq!(vec.len(), 20, "Should parse all 20 entries");
    }

    // Test 17: Error path - corrupted checksum file (invalid UTF-8)
    #[test]
    fn test_corrupted_checksum_file_utf8() {
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");

        // Write invalid UTF-8 bytes
        let invalid_utf8_data = vec![0xFF, 0xFE, 0xFD]; // Invalid UTF-8 sequence
        fs::write(checksum_file.path(), invalid_utf8_data).expect("Failed to write invalid UTF-8");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(result.is_err(), "Corrupted UTF-8 file should fail to parse");

        if let Err(CheckleError::InvalidChecksumFile(path)) = result {
            assert_eq!(path, checksum_file.path());
        } else {
            panic!("Expected InvalidChecksumFile error for UTF-8 corruption");
        }
    }

    // Test 18: Edge case - checksum file with mixed existing/non-existing files
    #[test]
    fn test_mixed_existing_nonexisting_files() {
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");
        let checksum_content = format!(
            "abcdef1234567890abcdef1234567890\t{}\n1234567890abcdef1234567890abcdef\t/nonexistent/file.txt\nfedcba0987654321fedcba0987654321\t{}",
            temp_file1.path().display(),
            temp_file2.path().display()
        );
        fs::write(checksum_file.path(), checksum_content)
            .expect("Failed to write mixed checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(
            result.is_ok(),
            "Mixed file existence should not cause total failure"
        );

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert_eq!(vec.len(), 2, "Should include only existing files"); // Only temp_file1 and temp_file2
    }

    // Test 19: Performance test - collect_files with many files in directory
    #[test]
    fn test_collect_files_many_files_performance() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_dir.path()).expect("Failed to change dir");

        // Create many small files
        let file_count = 50;
        let mut created_files = Vec::new();
        for i in 0..file_count {
            let file_name = format!("test_file_{:04}.txt", i);
            let file_path = temp_dir.path().join(&file_name);
            fs::write(&file_path, format!("content {}", i)).expect("Failed to write test file");
            created_files.push(file_path);
        }

        use std::time::Instant;
        let start = Instant::now();
        let result = collect_files(Path::new("*"), false);
        let duration = start.elapsed();

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        assert!(result.is_ok(), "Should collect many files successfully");
        let files = result.unwrap();
        assert_eq!(files.len(), file_count, "Should find all created files");

        // Should complete quickly (less than 1 second for 500 files)
        assert!(
            duration.as_secs() < 1,
            "File collection should be fast: {:?}",
            duration
        );
    }

    // Test 20: Edge case - file permissions and access errors simulation
    #[test]
    fn test_file_access_edge_cases() {
        // This test focuses on the FileHashPair validation behavior
        // We can't easily simulate permission errors in cross-platform tests,
        // but we can test the validation logic

        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test valid hash variations
        let valid_hashes = vec![
            "abc123def456",                     // lowercase hex
            "ABC123DEF456", // uppercase hex (should still work due to validation)
            "0123456789abcdef0123456789abcdef", // 32 char MD5-style
        ];

        for hash in valid_hashes {
            let pair = FileHashPair::new(temp_file.path().to_path_buf(), hash.to_string());
            assert_eq!(pair.hash(), hash, "Valid hash should be accepted");
        }
    }

    // Test 21: Edge case - checksum file with different line endings
    #[test]
    fn test_checksum_file_line_endings() {
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");
        let checksum_file = NamedTempFile::new().expect("Failed to create checksum file");

        // Test with Windows-style line endings (\r\n)
        let checksum_content = format!(
            "abcdef1234567890\t{}\r\n1234567890abcdef\t{}",
            temp_file1.path().display(),
            temp_file2.path().display()
        );
        fs::write(checksum_file.path(), checksum_content).expect("Failed to write checksum file");

        let result = FilesToCheck::new_from_txt(checksum_file.path());
        assert!(
            result.is_ok(),
            "Windows line endings should be handled correctly"
        );

        let files_to_check = result.unwrap();
        let vec = files_to_check.to_vec();
        assert_eq!(vec.len(), 2, "Should parse both entries with CRLF endings");
    }

    // Test 23: Stress test - FilesToCheck with maximum allowed files
    #[test]
    fn test_files_to_check_stress_max_files() {
        // Test close to the MAX_FILES_IN_BATCH limit (but not exceeding it)
        let batch_size = std::cmp::min(100, MAX_FILES_IN_BATCH / 100); // Reasonable size for tests
        let temp_files: Vec<_> = (0..batch_size)
            .map(|_| NamedTempFile::new().expect("Failed to create temp file"))
            .collect();

        let pairs: Vec<_> = temp_files
            .iter()
            .enumerate()
            .map(|(i, tf)| FileHashPair::new(tf.path().to_path_buf(), format!("{:032x}", i)))
            .collect();

        let files_to_check = FilesToCheck::from_vec(pairs);
        assert_eq!(
            files_to_check.0.len(),
            batch_size,
            "Should handle stress test batch size"
        );

        // Test conversion back and forth
        let vec = files_to_check.to_vec();
        assert_eq!(
            vec.len(),
            batch_size,
            "Round-trip conversion should preserve count"
        );
    }

    // Property-based tests
    proptest! {
        #![proptest_config({
            Config {
                failure_persistence: Some(Box::new(
                    FileFailurePersistence::SourceParallel("tests/proptest-regressions")
                )),
                ..Default::default()
            }
        })]
        // Property 1: FileHashPair creation with valid hex hashes
        #[test]
        fn test_file_hash_pair_with_valid_hex_hashes(hash_bytes in prop::collection::vec(any::<u8>(), 16..32)) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            let hex_hash = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

            let pair = FileHashPair::new(temp_file.path().to_path_buf(), hex_hash.clone());
            prop_assert_eq!(pair.hash(), &hex_hash);
            prop_assert_eq!(pair.file(), temp_file.path());
        }

        // Property 2: FilesToCheck operations maintain invariants
        #[test]
        fn test_files_to_check_invariants(count in 1usize..100) {
            let temp_files: Vec<_> = (0..count).map(|_| NamedTempFile::new().expect("Failed to create temp file")).collect();
            let pairs: Vec<_> = temp_files.iter().enumerate().map(|(i, tf)| {
                FileHashPair::new(tf.path().to_path_buf(), format!("{:032x}", i))
            }).collect();

            let files_to_check = FilesToCheck::from_vec(pairs.clone());
            prop_assert_eq!(files_to_check.0.len(), count);

            let retrieved = files_to_check.to_vec();
            prop_assert_eq!(retrieved.len(), count);
        }

        // Property 3: collect_files with single file always returns one element
        #[test]
        fn test_collect_files_single_file_property(_data in prop::collection::vec(any::<u8>(), 0..1000)) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");

            let result = collect_files(temp_file.path(), false);
            prop_assert!(result.is_ok());

            let files = result.unwrap();
            prop_assert_eq!(files.len(), 1);
            prop_assert_eq!(&files[0], temp_file.path());
        }

        // Property 4: Hash validation is consistent
        #[test]
        fn test_hash_validation_consistency(valid_hex_chars in "[0-9a-fA-F]{16,64}") {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");

            // All valid hex strings should be accepted
            let pair = FileHashPair::new(temp_file.path().to_path_buf(), valid_hex_chars.clone());
            prop_assert_eq!(pair.hash(), &valid_hex_chars);
        }

        // Property 5: FilesToCheck maintains size constraints
        #[test]
        fn test_files_to_check_size_constraints(count in 1usize..50) {
            let temp_files: Vec<_> = (0..count).map(|_| NamedTempFile::new().expect("Failed to create temp file")).collect();
            let pairs: Vec<_> = temp_files.iter().enumerate().map(|(i, tf)| {
                FileHashPair::new(tf.path().to_path_buf(), format!("{:032x}", i))
            }).collect();

            let files_to_check = FilesToCheck::from_vec(pairs);
            prop_assert!(files_to_check.0.len() <= MAX_FILES_IN_BATCH);
            prop_assert_eq!(files_to_check.0.len(), count);
        }

        // Property 6: Path handling is robust
        #[test]
        fn test_path_handling_robustness(
            filename in "[a-zA-Z0-9_-]{1,50}\\.(txt|bin|dat)"
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let file_path = temp_dir.path().join(&filename);
            fs::write(&file_path, b"test content").expect("Failed to write test file");

            let pair = FileHashPair::new(file_path.clone(), "0123456789abcdef0123456789abcdef".to_string());
            prop_assert_eq!(pair.file(), file_path.as_path());
        }
    }

    // Test 23: Test recursive file collection
    #[test]
    fn test_collect_files_recursive() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create nested directory structure
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).expect("Failed to create subdirectory");

        let nested_dir = sub_dir.join("nested");
        fs::create_dir(&nested_dir).expect("Failed to create nested directory");

        // Create files at different levels
        let root_file = temp_dir.path().join("root.txt");
        fs::write(&root_file, b"root content").expect("Failed to write root file");

        let sub_file = sub_dir.join("sub.txt");
        fs::write(&sub_file, b"sub content").expect("Failed to write sub file");

        let nested_file = nested_dir.join("nested.txt");
        fs::write(&nested_file, b"nested content").expect("Failed to write nested file");

        // Test recursive collection
        let files =
            collect_files(temp_dir.path(), true).expect("Recursive collection should succeed");
        assert_eq!(files.len(), 3, "Should find all 3 files recursively");

        // Files should be found at all levels
        assert!(files.contains(&root_file), "Should find root file");
        assert!(files.contains(&sub_file), "Should find subdirectory file");
        assert!(files.contains(&nested_file), "Should find nested file");
    }

    // Test 24: Test non-recursive file collection
    #[test]
    fn test_collect_files_non_recursive() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create nested directory structure
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).expect("Failed to create subdirectory");

        // Create files at different levels
        let root_file1 = temp_dir.path().join("root1.txt");
        fs::write(&root_file1, b"content1").expect("Failed to write file");

        let root_file2 = temp_dir.path().join("root2.txt");
        fs::write(&root_file2, b"content2").expect("Failed to write file");

        let sub_file = sub_dir.join("sub.txt");
        fs::write(&sub_file, b"sub content").expect("Failed to write sub file");

        // Test non-recursive collection
        let files =
            collect_files(temp_dir.path(), false).expect("Non-recursive collection should succeed");
        assert_eq!(files.len(), 2, "Should find only 2 root files");

        // Should only find root level files
        assert!(files.contains(&root_file1), "Should find first root file");
        assert!(files.contains(&root_file2), "Should find second root file");
        assert!(
            !files.contains(&sub_file),
            "Should not find subdirectory file"
        );
    }

    // Test 25: Test recursive with hidden files
    #[test]
    fn test_collect_files_recursive_hidden_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create normal and hidden files
        let normal_file = temp_dir.path().join("normal.txt");
        fs::write(&normal_file, b"normal").expect("Failed to write normal file");

        let hidden_file = temp_dir.path().join(".hidden");
        fs::write(&hidden_file, b"hidden").expect("Failed to write hidden file");

        // Create a hidden directory with a file
        let hidden_dir = temp_dir.path().join(".config");
        fs::create_dir(&hidden_dir).expect("Failed to create hidden directory");

        let file_in_hidden = hidden_dir.join("config.txt");
        fs::write(&file_in_hidden, b"config").expect("Failed to write file in hidden dir");

        // Test recursive collection
        let files = collect_files(temp_dir.path(), true).expect("Collection should succeed");

        // Should find normal file
        assert!(files.contains(&normal_file), "Should find normal file");

        // Should skip hidden files and directories by default
        assert!(!files.contains(&hidden_file), "Should skip hidden file");
        assert!(
            !files.contains(&file_in_hidden),
            "Should skip file in hidden directory"
        );
    }

    // Test 26: Test recursive depth limit
    #[test]
    fn test_collect_files_recursive_depth_limit() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a deep directory structure
        let mut current_dir = temp_dir.path().to_path_buf();
        for i in 0..10 {
            current_dir = current_dir.join(format!("level{}", i));
            fs::create_dir(&current_dir).expect("Failed to create directory");

            let file = current_dir.join(format!("file{}.txt", i));
            fs::write(&file, format!("content at level {}", i)).expect("Failed to write file");
        }

        // Test recursive collection
        let files = collect_files(temp_dir.path(), true).expect("Collection should succeed");

        // Should find files up to MAX_RECURSION_DEPTH
        assert!(!files.is_empty(), "Should find some files");
        assert!(files.len() <= 10, "Should not exceed directory count");
    }
}
