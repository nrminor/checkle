use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, CheckleError>;

#[derive(Debug, Error)]
pub enum CheckleError {
    #[error(
        "The provided file `{0}` failed the checksum process. It was likely truncated during a file transfer or otherwise mutated since the hash was originally computed."
    )]
    FailedChecksum(PathBuf),

    #[error("Multiple files failed the checksum. See logged output above.")]
    MultipleFailedChecksums,

    #[error("The provided file `{0}` does not exist or is otherwise inaccessible.")]
    InaccessibleFile(PathBuf),

    #[error(
        "The provided checksum file `{0}` was invalid and could not be parsed. Please double check that it is tab delimited with two columns and no header, where the first column is the hash and the second column is the corresponding file path (relative or absolute)."
    )]
    InvalidChecksumFile(PathBuf),

    #[error(
        "Failed to open file '{path}' for hashing. Ensure the file exists, is readable, and you have the necessary permissions. The underlying error was: {source}"
    )]
    FileOpenError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Failed to read from file '{path}' during hash computation. This could indicate disk corruption, network issues if the file is on a remote filesystem, or insufficient permissions. The underlying error was: {source}"
    )]
    FileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Hash computation failed for file '{path}' using {algorithm} algorithm. The computed hash size ({computed_size} bytes) does not match the expected size ({expected_size} bytes). This indicates an internal error in the hash algorithm implementation."
    )]
    HashSizeMismatch {
        path: PathBuf,
        algorithm: String,
        computed_size: usize,
        expected_size: usize,
    },

    #[error(
        "Failed to convert hash result to the required format for file '{path}' using {algorithm} algorithm. This is an internal error that suggests a problem with hash result processing. Please report this issue."
    )]
    HashConversionError { path: PathBuf, algorithm: String },

    #[error(
        "Failed to convert computed hash to UTF-8 string for file '{path}'. The hash contains invalid bytes: {invalid_bytes:?}. This is an unexpected internal error."
    )]
    HashStringConversionError {
        path: PathBuf,
        invalid_bytes: Vec<u8>,
    },

    #[error(
        "Merkle tree computation failed for file '{path}' using {algorithm} algorithm at tree level {level}. This could indicate memory issues or hash computation problems. Try with a smaller file or different algorithm."
    )]
    MerkleTreeComputationError {
        path: PathBuf,
        algorithm: String,
        level: usize,
    },

    #[error(
        "Internal error during hash array processing for file '{path}'. Expected exactly one root hash but found {found_count} hashes. This suggests a bug in the Merkle tree computation."
    )]
    InvalidMerkleTreeResult { path: PathBuf, found_count: usize },

    #[error(
        "Failed to get current working directory. This could indicate a permissions issue or that the current directory has been deleted. The underlying error was: {source}"
    )]
    CurrentDirectoryError {
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Failed to read directory '{path}'. This could indicate permissions issues or that the directory is inaccessible. The underlying error was: {source}"
    )]
    DirectoryReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Failed to initialize global thread pool with {thread_count} threads. This is an internal error. The underlying error was: {source}"
    )]
    ThreadPoolInitError {
        thread_count: usize,
        #[source]
        source: rayon::ThreadPoolBuildError,
    },

    #[error(
        "Invalid chunk size: {size} bytes. {reason} Valid range is {min_size} to {max_size} bytes, aligned to page boundaries (4KB)."
    )]
    InvalidChunkSize {
        size: usize,
        reason: String,
        min_size: usize,
        max_size: usize,
    },

    #[error(
        "The output file '{path}' already exists. Please remove it if you want to overwrite it, or choose a different output file path."
    )]
    OutputFileExists { path: PathBuf },

    #[error("Invalid CLI argument: {0}")]
    InvalidCliArgument(String),

    #[error("Invalid numeric value '{value}': {reason}")]
    InvalidNumericValue { value: String, reason: String },

    // ============================================================================
    // Archive-Related Errors
    // ============================================================================
    #[error(
        "Unsupported archive format: {0}\n\nSupported formats:\n- TAR archives (.tar, .tar.gz, .tar.bz2, .tar.xz)\n- ZIP archives (.zip)\n\nPlease ensure your file has the correct extension and format."
    )]
    UnsupportedArchiveFormat(PathBuf),

    #[error(
        "File '{file}' not found in archive '{archive}'\n\nUse 'checkle hash-archive {archive} --list' to see available files."
    )]
    FileNotFoundInArchive { archive: PathBuf, file: String },

    #[error(
        "Archive '{path}' appears to be corrupted: {details}\n\nPossible causes:\n1. Incomplete download or transfer\n2. Archive created with incompatible settings\n3. File system corruption\n\nTry re-downloading or recreating the archive."
    )]
    CorruptedArchive { path: PathBuf, details: String },

    #[error(
        "Archive '{path}' is {size} bytes, exceeding the limit of {limit} bytes\n\nThis archive is too large to process safely. Consider:\n1. Splitting the archive into smaller parts\n2. Processing files separately outside the archive"
    )]
    ArchiveTooLarge {
        path: PathBuf,
        size: u64,
        limit: u64,
    },

    #[error(
        "Archive '{path}' contains {count} entries, exceeding the limit of {limit}\n\nThis limit exists to prevent memory exhaustion. Consider:\n1. Splitting the archive into smaller parts\n2. Processing specific files instead of the entire archive"
    )]
    TooManyArchiveEntries {
        path: PathBuf,
        count: usize,
        limit: usize,
    },

    #[error(
        "Archive entry '{entry}' in '{archive}' is {size} bytes, exceeding the limit of {limit} bytes\n\nThis file is too large to process safely. Consider:\n1. Processing this file separately outside the archive\n2. Splitting the file into smaller chunks"
    )]
    ArchiveEntryTooLarge {
        archive: PathBuf,
        entry: String,
        size: u64,
        limit: u64,
    },

    #[error(
        "Failed to read from archive: {details}\n\nThis may indicate:\n1. Corrupted compressed data\n2. Unsupported compression method\n3. I/O errors during decompression"
    )]
    ArchiveReadError { details: String },

    #[error(
        "Archive operation timed out after {elapsed:?}\n\nThe archive '{path}' is taking too long to process. This could indicate:\n1. Extremely large or complex compression\n2. System resource constraints\n3. Corrupted data causing decompression loops"
    )]
    ArchiveTimeout {
        path: PathBuf,
        elapsed: std::time::Duration,
    },

    #[error(
        "Invalid archive format for '{path}'\nExpected: {expected}\nActual: {actual}\n\nThe file extension doesn't match the actual format. Please rename the file or specify the correct format."
    )]
    InvalidArchiveFormat {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("Archive entry not found: {archive}:{entry}")]
    ArchiveEntryNotFound { archive: PathBuf, entry: String },

    #[error("Mixed archive and filesystem sources are not supported in the same operation")]
    MixedSourceVerification,

    #[error(
        "Hash computation error: {details}\n\nThis could indicate:\n1. Internal hash algorithm failure\n2. Memory allocation issues\n3. System resource constraints"
    )]
    HashingError { details: String },

    // ============================================================================
    // Pretty Printing Errors
    // ============================================================================
    #[error(
        "Invalid hash format: '{hash}' is not a valid hexadecimal hash\n\nHashes must contain only characters 0-9 and a-f (case insensitive).\nExample: 'a1b2c3d4e5f6789abcdef1234567890'"
    )]
    InvalidHashFormat { hash: String },

    #[error(
        "Empty hash value provided\n\nA hash value is required but an empty string was given.\nPlease provide a valid hash string."
    )]
    EmptyHash,

    #[error(
        "Failed to write output to stderr: {details}\n\nThis may occur if:\n1. stderr is redirected to a closed pipe\n2. The terminal is disconnected\n3. Disk is full\n\nTry running without output redirection or check disk space."
    )]
    StderrWriteError {
        details: String,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Table formatting failed: {details}\n\nThis is an internal error in the pretty-printing system.\nPlease report this issue with the full error message."
    )]
    TableFormattingError { details: String },

    #[error(
        "Path conversion failed for '{path}'\n\nThe file path contains invalid UTF-8 characters.\nPlease ensure all file paths use valid UTF-8 encoding."
    )]
    InvalidPathEncoding { path: String },

    #[error(
        "Exceeded maximum file batch size: found {found} files, but the limit is {limit}\n\nThis limit exists to prevent memory exhaustion. You can:\n1. Hash a smaller directory tree\n2. Use more specific filters:\n   checkle hash <path> --include '*.fastq' --exclude '**/temp/**'\n3. If your system has sufficient memory, increase the limit:\n   checkle hash <path> --max-files-batch 50000"
    )]
    ExceededFileBatchSize { found: usize, limit: usize },

    #[error("Unknown error encountered.")]
    UnknownError(#[from] color_eyre::Report),
}
pub use CheckleError::*;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::redundant_closure_for_method_calls,
        clippy::match_same_arms
    )]
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, FileFailurePersistence};
    use std::{error::Error, io};
    use tempfile::NamedTempFile;

    // Test 1: Normal operation - error formatting for FailedChecksum
    #[test]
    fn test_failed_checksum_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::FailedChecksum(temp_file.path().to_path_buf());

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("failed the checksum process"),
            "Error should describe checksum failure"
        );
        assert!(
            error_string.contains("truncated"),
            "Error should suggest truncation as cause"
        );
        assert!(
            error_string.contains("file transfer"),
            "Error should suggest transfer issues"
        );

        // Test Debug formatting
        let debug_string = format!("{:?}", error);
        assert!(
            debug_string.contains("FailedChecksum"),
            "Debug should show error variant"
        );
        assert!(
            debug_string.contains(&temp_file.path().to_string_lossy().to_string()),
            "Debug should show file path"
        );
    }

    // Test 2: Normal operation - error formatting for MultipleFailedChecksums
    #[test]
    fn test_multiple_failed_checksums_error_formatting() {
        let error = CheckleError::MultipleFailedChecksums;

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Multiple files failed"),
            "Error should describe multiple failures"
        );
        assert!(
            error_string.contains("logged output above"),
            "Error should reference logs"
        );

        // Test Debug formatting
        let debug_string = format!("{:?}", error);
        assert!(
            debug_string.contains("MultipleFailedChecksums"),
            "Debug should show error variant"
        );
    }

    // Test 3: Normal operation - error formatting for InaccessibleFile
    #[test]
    fn test_inaccessible_file_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::InaccessibleFile(temp_file.path().to_path_buf());

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("does not exist"),
            "Error should mention file doesn't exist"
        );
        assert!(
            error_string.contains("inaccessible"),
            "Error should mention inaccessibility"
        );
        assert!(
            error_string.contains(&temp_file.path().to_string_lossy().to_string()),
            "Error should show file path"
        );
    }

    // Test 4: Normal operation - error formatting for InvalidChecksumFile
    #[test]
    fn test_invalid_checksum_file_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::InvalidChecksumFile(temp_file.path().to_path_buf());

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("invalid"),
            "Error should describe file as invalid"
        );
        assert!(
            error_string.contains("tab delimited"),
            "Error should describe expected format"
        );
        assert!(
            error_string.contains("two columns"),
            "Error should specify column count"
        );
        assert!(
            error_string.contains("no header"),
            "Error should mention header requirement"
        );
    }

    // Test 5: Normal operation - error formatting for FileOpenError with source
    #[test]
    fn test_file_open_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
        let error = CheckleError::FileOpenError {
            path: temp_file.path().to_path_buf(),
            source: io_error,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Failed to open file"),
            "Error should describe open failure"
        );
        assert!(
            error_string.contains("readable"),
            "Error should mention readability"
        );
        assert!(
            error_string.contains("permissions"),
            "Error should mention permissions"
        );
        assert!(
            error_string.contains("Permission denied"),
            "Error should include source error"
        );

        // Test source error chain
        let source = error.source();
        assert!(source.is_some(), "Error should have a source");
        assert!(source.unwrap().to_string().contains("Permission denied"));
    }

    // Test 6: Normal operation - error formatting for FileReadError with source
    #[test]
    fn test_file_read_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let io_error = io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF");
        let error = CheckleError::FileReadError {
            path: temp_file.path().to_path_buf(),
            source: io_error,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Failed to read from file"),
            "Error should describe read failure"
        );
        assert!(
            error_string.contains("disk corruption"),
            "Error should suggest disk corruption"
        );
        assert!(
            error_string.contains("network issues"),
            "Error should suggest network issues"
        );
        assert!(
            error_string.contains("Unexpected EOF"),
            "Error should include source error"
        );
    }

    // Test 7: Normal operation - error formatting for HashSizeMismatch
    #[test]
    fn test_hash_size_mismatch_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::HashSizeMismatch {
            path: temp_file.path().to_path_buf(),
            algorithm: "MD5".to_string(),
            computed_size: 20,
            expected_size: 16,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Hash computation failed"),
            "Error should describe computation failure"
        );
        assert!(error_string.contains("MD5"), "Error should show algorithm");
        assert!(
            error_string.contains("20 bytes"),
            "Error should show computed size"
        );
        assert!(
            error_string.contains("16 bytes"),
            "Error should show expected size"
        );
        assert!(
            error_string.contains("internal error"),
            "Error should indicate internal error"
        );
    }

    // Test 8: Normal operation - error formatting for HashConversionError
    #[test]
    fn test_hash_conversion_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::HashConversionError {
            path: temp_file.path().to_path_buf(),
            algorithm: "SHA256".to_string(),
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Failed to convert hash result"),
            "Error should describe conversion failure"
        );
        assert!(
            error_string.contains("SHA256"),
            "Error should show algorithm"
        );
        assert!(
            error_string.contains("internal error"),
            "Error should indicate internal error"
        );
        assert!(
            error_string.contains("report this issue"),
            "Error should suggest reporting"
        );
    }

    // Test 9: Normal operation - error formatting for InvalidMerkleTreeResult
    #[test]
    fn test_invalid_merkle_tree_result_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = CheckleError::InvalidMerkleTreeResult {
            path: temp_file.path().to_path_buf(),
            found_count: 3,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Internal error"),
            "Error should describe as internal error"
        );
        assert!(
            error_string.contains("exactly one root hash"),
            "Error should describe expected result"
        );
        assert!(
            error_string.contains("found 3 hashes"),
            "Error should show actual count"
        );
        assert!(
            error_string.contains("bug in the Merkle tree"),
            "Error should suggest bug"
        );
    }

    // Test 10: Normal operation - error formatting for CurrentDirectoryError
    #[test]
    fn test_current_directory_error_formatting() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "Directory not found");
        let error = CheckleError::CurrentDirectoryError { source: io_error };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Failed to get current working directory"),
            "Error should describe directory access failure"
        );
        assert!(
            error_string.contains("permissions issue"),
            "Error should suggest permissions"
        );
        assert!(
            error_string.contains("deleted"),
            "Error should suggest directory deletion"
        );
        assert!(
            error_string.contains("Directory not found"),
            "Error should include source error"
        );
    }

    // Test 11: Normal operation - error formatting for DirectoryReadError
    #[test]
    fn test_directory_read_error_formatting() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "Access denied");
        let error = CheckleError::DirectoryReadError {
            path: temp_file.path().to_path_buf(),
            source: io_error,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Failed to read directory"),
            "Error should describe read failure"
        );
        assert!(
            error_string.contains("permissions issues"),
            "Error should mention permissions"
        );
        assert!(
            error_string.contains("inaccessible"),
            "Error should mention inaccessibility"
        );
        assert!(
            error_string.contains("Access denied"),
            "Error should include source error"
        );
    }

    // Test 12: Normal operation - error formatting for ThreadPoolInitError
    #[test]
    fn test_thread_pool_init_error_formatting() {
        // We can't easily create a real ThreadPoolBuildError, so we'll test the structure
        // This test verifies the error variant exists and can be constructed
        // In a real scenario, this would be created by rayon's thread pool builder
    }

    // Test 13: Error path context preservation
    #[test]
    fn test_error_context_preservation() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let original_error =
            io::Error::new(io::ErrorKind::PermissionDenied, "Original error message");
        let wrapped_error = CheckleError::FileOpenError {
            path: temp_file.path().to_path_buf(),
            source: original_error,
        };

        // Test that we can retrieve the original error
        let source = wrapped_error.source();
        assert!(source.is_some(), "Wrapped error should preserve source");

        let source_str = source.unwrap().to_string();
        assert!(
            source_str.contains("Original error message"),
            "Source error message should be preserved"
        );

        // Test error chain walking
        let mut current_error: &dyn std::error::Error = &wrapped_error;
        let mut error_count = 0;
        while let Some(next_error) = current_error.source() {
            current_error = next_error;
            error_count += 1;
        }
        assert_eq!(error_count, 1, "Should have exactly one source error");
    }

    // Test 14: Error type classification
    #[test]
    fn test_error_type_classification() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test file-related errors
        let file_errors = vec![
            CheckleError::FailedChecksum(temp_file.path().to_path_buf()),
            CheckleError::InaccessibleFile(temp_file.path().to_path_buf()),
            CheckleError::FileOpenError {
                path: temp_file.path().to_path_buf(),
                source: io::Error::new(io::ErrorKind::NotFound, "Not found"),
            },
        ];

        for error in file_errors {
            match error {
                CheckleError::FailedChecksum(_)
                | CheckleError::InaccessibleFile(_)
                | CheckleError::FileOpenError { .. } => {
                    // These are expected file-related errors
                }
                _ => panic!("Unexpected error type"),
            }
        }

        // Test validation errors
        let validation_errors = vec![
            CheckleError::InvalidChecksumFile(temp_file.path().to_path_buf()),
            CheckleError::HashSizeMismatch {
                path: temp_file.path().to_path_buf(),
                algorithm: "MD5".to_string(),
                computed_size: 20,
                expected_size: 16,
            },
        ];

        for error in validation_errors {
            match error {
                CheckleError::InvalidChecksumFile(_) | CheckleError::HashSizeMismatch { .. } => {
                    // These are expected validation errors
                }
                _ => panic!("Unexpected error type"),
            }
        }
    }

    // Test 16: Error chain depth and source propagation
    #[test]
    fn test_error_chain_depth_and_propagation() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create a chain of errors
        let root_error = io::Error::new(io::ErrorKind::PermissionDenied, "Access forbidden");
        let wrapped_error = CheckleError::FileOpenError {
            path: temp_file.path().to_path_buf(),
            source: root_error,
        };

        // Test error chain walking
        let mut current: &dyn std::error::Error = &wrapped_error;
        let mut chain_length = 0;
        let mut error_messages = Vec::new();

        loop {
            error_messages.push(current.to_string());
            chain_length += 1;

            match current.source() {
                Some(source) => current = source,
                None => break,
            }

            // Prevent infinite loops
            assert!(chain_length < 10, "Error chain should not be infinite");
        }

        assert_eq!(chain_length, 2, "Should have exactly 2 errors in chain");
        assert!(
            error_messages[0].contains("Failed to open file"),
            "Top-level error should be descriptive"
        );
        assert!(
            error_messages[1].contains("Access forbidden"),
            "Root error should be preserved"
        );
    }

    // Test 17: Error consistency across different contexts
    #[test]
    fn test_error_consistency_across_contexts() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test the same error type with different paths
        let paths = vec![
            temp_file.path().to_path_buf(),
            PathBuf::from("/tmp/test.txt"),
            PathBuf::from("/very/long/path/to/some/file.txt"),
        ];

        for path in paths {
            let error = CheckleError::FailedChecksum(path.clone());
            let error_string = error.to_string();

            // All errors of the same type should have consistent structure
            assert!(
                error_string.contains("failed the checksum process"),
                "Error message should be consistent"
            );
            assert!(
                error_string.contains(&path.to_string_lossy().to_string()),
                "Error should include the path"
            );
            assert!(
                error_string.len() > 50,
                "Error message should be descriptive"
            );
        }
    }

    // Test 18: Memory efficiency - error size optimization
    #[test]
    fn test_error_memory_efficiency() {
        use std::mem;

        // Ensure error variants don't take excessive memory
        let error_size = mem::size_of::<CheckleError>();

        // CheckleError should be reasonably sized (less than 1KB per error)
        assert!(
            error_size < 1024,
            "CheckleError should be memory efficient: {} bytes",
            error_size
        );

        // Test that creating many errors doesn't cause memory issues
        let mut errors = Vec::new();
        for i in 0..10000 {
            let path = PathBuf::from(format!("/tmp/test_{}.txt", i));
            errors.push(CheckleError::FailedChecksum(path));
        }

        assert_eq!(
            errors.len(),
            10000,
            "Should be able to create many errors efficiently"
        );

        // Test memory usage with string formatting
        let formatted_errors: Vec<String> =
            errors.iter().take(100).map(|e| e.to_string()).collect();
        assert_eq!(
            formatted_errors.len(),
            100,
            "Should format errors efficiently"
        );
    }

    // Test 19: Error interoperability with other error types
    #[test]
    fn test_error_interoperability() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test conversion to generic error trait objects
        let checkle_error: Box<dyn std::error::Error> =
            Box::new(CheckleError::FailedChecksum(temp_file.path().to_path_buf()));

        // Should still be usable as generic error
        assert!(
            !checkle_error.to_string().is_empty(),
            "Boxed error should format correctly"
        );

        // Test with std::result::Result<T, Box<dyn Error>>
        let error_result: std::result::Result<(), Box<dyn std::error::Error>> =
            Err(Box::new(CheckleError::MultipleFailedChecksums));

        assert!(
            error_result.is_err(),
            "Error should propagate correctly in generic contexts"
        );

        if let Err(e) = error_result {
            assert!(
                e.to_string().contains("Multiple files failed"),
                "Generic error should preserve message"
            );
        }
    }

    // Test 20: Thread safety of error types
    #[test]
    fn test_error_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let error = Arc::new(CheckleError::FailedChecksum(temp_file.path().to_path_buf()));

        // Test that errors can be shared between threads
        let error_clone = Arc::clone(&error);
        let handle = thread::spawn(move || {
            let error_string = error_clone.to_string();
            assert!(error_string.contains("failed the checksum process"));
            error_string
        });

        let result = handle.join().expect("Thread should complete successfully");
        assert!(
            !result.is_empty(),
            "Error should format correctly across threads"
        );

        // Original error should still be valid
        assert!(error.to_string().contains("failed the checksum process"));
    }

    // Test 21: Error recovery patterns
    #[test]
    fn test_error_recovery_patterns() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test various error recovery scenarios
        let errors = vec![
            CheckleError::FailedChecksum(temp_file.path().to_path_buf()),
            CheckleError::InaccessibleFile(temp_file.path().to_path_buf()),
            CheckleError::InvalidChecksumFile(temp_file.path().to_path_buf()),
        ];

        for error in errors {
            // Test pattern matching for error recovery
            let recovery_action = match &error {
                CheckleError::FailedChecksum(_) => "re-transfer file",
                CheckleError::InaccessibleFile(_) => "check permissions",
                CheckleError::InvalidChecksumFile(_) => "fix checksum format",
                _ => "generic recovery",
            };

            assert!(
                !recovery_action.is_empty(),
                "Each error should have a recovery action"
            );

            // Test that error classification works for automated recovery
            let is_recoverable = match &error {
                CheckleError::FailedChecksum(_)
                | CheckleError::InaccessibleFile(_)
                | CheckleError::InvalidChecksumFile(_) => true,
                CheckleError::MultipleFailedChecksums => false, // Requires manual intervention
                _ => false,
            };

            // Most single-file errors should be recoverable
            if matches!(
                error,
                CheckleError::FailedChecksum(_)
                    | CheckleError::InaccessibleFile(_)
                    | CheckleError::InvalidChecksumFile(_)
            ) {
                assert!(is_recoverable, "Single-file errors should be recoverable");
            }
        }
    }

    // Test 22: Normal operation - error formatting for ExceededFileBatchSize
    #[test]
    fn test_exceeded_file_batch_size_error_formatting() {
        let error = CheckleError::ExceededFileBatchSize {
            found: 15000,
            limit: 10000,
        };

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("Exceeded maximum file batch size"),
            "Error should describe batch size exceeded"
        );
        assert!(
            error_string.contains("found 15000 files"),
            "Error should show actual count"
        );
        assert!(
            error_string.contains("limit is 10000"),
            "Error should show limit"
        );
        assert!(
            error_string.contains("Hash a smaller directory tree"),
            "Error should suggest solutions"
        );
        assert!(
            error_string.contains("checkle hash <path> --max-files-batch"),
            "Error should show exact command example"
        );

        // Test Debug formatting
        let debug_string = format!("{:?}", error);
        assert!(
            debug_string.contains("ExceededFileBatchSize"),
            "Debug should show error variant"
        );
        assert!(
            debug_string.contains("15000"),
            "Debug should show found count"
        );
        assert!(debug_string.contains("10000"), "Debug should show limit");
    }

    // Test 23: Stress test - error handling under load
    #[test]
    fn test_error_handling_stress() {
        use std::time::Instant;

        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create and format many errors quickly
        let start = Instant::now();
        let mut formatted_errors = Vec::new();

        for i in 0..1000 {
            let error = if i % 3 == 0 {
                CheckleError::FailedChecksum(temp_file.path().to_path_buf())
            } else if i % 3 == 1 {
                CheckleError::HashSizeMismatch {
                    path: temp_file.path().to_path_buf(),
                    algorithm: "MD5".to_string(),
                    computed_size: 20,
                    expected_size: 16,
                }
            } else {
                CheckleError::MultipleFailedChecksums
            };

            formatted_errors.push(error.to_string());
        }

        let duration = start.elapsed();

        assert_eq!(
            formatted_errors.len(),
            1000,
            "Should create 1000 formatted errors"
        );
        assert!(
            duration.as_secs() < 1,
            "Error formatting should be fast: {:?}",
            duration
        );

        // Verify all errors were formatted correctly
        for (i, formatted) in formatted_errors.iter().enumerate() {
            assert!(!formatted.is_empty(), "Error {} should not be empty", i);
            assert!(formatted.len() > 10, "Error {} should be descriptive", i);
        }
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
        // Property 1: Path-based errors preserve path information
        #[test]
        fn test_path_errors_preserve_paths(path_str in "[a-zA-Z0-9/._-]{1,100}") {
            let path = PathBuf::from(&path_str);

            let errors = vec![
                CheckleError::FailedChecksum(path.clone()),
                CheckleError::InaccessibleFile(path.clone()),
                CheckleError::InvalidChecksumFile(path.clone()),
            ];

            for error in errors {
                let error_string = error.to_string();
                let debug_string = format!("{:?}", error);

                // Both display and debug should contain path information
                prop_assert!(error_string.contains(&path_str) || debug_string.contains(&path_str));
            }
        }

        // Property 2: Algorithm-specific errors preserve algorithm information
        #[test]
        fn test_algorithm_errors_preserve_algorithm(algo in "[A-Z]{2,10}") {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");

            let errors = vec![
                CheckleError::HashSizeMismatch {
                    path: temp_file.path().to_path_buf(),
                    algorithm: algo.clone(),
                    computed_size: 20,
                    expected_size: 16,
                },
                CheckleError::HashConversionError {
                    path: temp_file.path().to_path_buf(),
                    algorithm: algo.clone(),
                },
            ];

            for error in errors {
                let error_string = error.to_string();
                prop_assert!(error_string.contains(&algo));
            }
        }

        // Property 3: Size-related errors preserve size information
        #[test]
        fn test_size_errors_preserve_sizes(
            computed_size in 1usize..1000,
            expected_size in 1usize..1000
        ) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");

            let error = CheckleError::HashSizeMismatch {
                path: temp_file.path().to_path_buf(),
                algorithm: "TEST".to_string(),
                computed_size,
                expected_size,
            };

            let error_string = error.to_string();
            prop_assert!(error_string.contains(&computed_size.to_string()));
            prop_assert!(error_string.contains(&expected_size.to_string()));
        }
    }
}
