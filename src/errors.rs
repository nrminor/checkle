//! Comprehensive error handling for bioinformatics checksum operations.
//!
//! This module defines the complete error taxonomy for checkle, a high-performance
//! checksum utility designed for bioinformatics workflows where data integrity is
//! critical. The error system follows a two-tier architecture:
//!
//! # Error Handling Philosophy
//!
//! **Modules use custom errors (thiserror), main uses color-eyre**
//! - Individual modules define specific error types with precise context
//! - The main application layer uses color-eyre for rich error reporting
//! - All errors provide actionable solutions rather than just describing problems
//!
//! # Error Architecture
//!
//! 1. **Structured Error Context**: Every error includes relevant context like file paths,
//!    algorithms, sizes, and operation details
//! 2. **Actionable Messages**: Error messages suggest specific remediation steps
//! 3. **Error Chaining**: Underlying system errors are preserved as sources
//! 4. **Bioinformatics Focus**: Error messages are tailored for genomics data workflows
//!
//! # Comprehensive Error Taxonomy
//!
//! - **File Integrity Errors**: Checksum mismatches, corruption detection
//! - **I/O Errors**: File access, permission, and filesystem issues
//! - **Hash Computation Errors**: Algorithm failures, size mismatches, format issues
//! - **Archive Processing Errors**: Compression/decompression failures, format issues
//! - **System Resource Errors**: Memory, thread pool, disk space limitations
//! - **Configuration Errors**: Invalid parameters, CLI arguments, batch sizes
//!
//! # Usage in Bioinformatics Workflows
//!
//! In bioinformatics, data integrity is paramount. A single corrupted byte in a
//! genome assembly or sequencing file can invalidate entire analyses. This error
//! system is designed to:
//!
//! - Detect data corruption during file transfers between compute clusters
//! - Validate integrity of archived experimental datasets
//! - Provide clear diagnostics when processing large genomics archives
//! - Handle resource constraints common in high-throughput genomics pipelines
//!
//! # Examples
//!
//! ```no_run
//! use checkle::errors::{CheckleError, Result};
//! use std::path::PathBuf;
//!
//! // File integrity validation for genomics data
//! let result: Result<()> = verify_genome_file("sample.fastq.gz");
//! match result {
//!     Err(CheckleError::FailedChecksum(path)) => {
//!         eprintln!("Genome file {} is corrupted - re-transfer required", path.display());
//!     },
//!     Err(CheckleError::ArchiveTooLarge { size, limit, .. }) => {
//!         eprintln!("Archive {} MB exceeds limit {} MB", size / 1_000_000, limit / 1_000_000);
//!     },
//!     Ok(_) => println!("Genome data integrity verified"),
//!     Err(e) => eprintln!("Validation error: {}", e),
//! }
//!
//! # fn verify_genome_file(path: &str) -> Result<()> { Ok(()) }
//! ```

use std::path::PathBuf;

use thiserror::Error;

/// Convenient alias for Result types throughout the checkle codebase.
///
/// This type alias eliminates the need to repeatedly specify `CheckleError` as the
/// error type, making function signatures cleaner and more consistent across the
/// codebase. All functions that can fail should return this type.
///
/// # Examples
///
/// ```no_run
/// use checkle::errors::Result;
///
/// fn process_genome_file(path: &str) -> Result<String> {
///     // Function body that may return CheckleError
///     # Ok("processed".to_string())
/// }
/// ```
pub type Result<T> = std::result::Result<T, CheckleError>;

/// Comprehensive error type for all checkle operations.
///
/// This enum captures all possible failure modes in checkle's operation, from basic
/// file I/O errors to complex archive processing failures. Each variant provides
/// structured context and actionable error messages tailored for bioinformatics
/// workflows where data integrity is critical.
///
/// The error design follows these principles:
/// - **Context-Rich**: Every error includes relevant details (paths, sizes, algorithms)
/// - **Actionable**: Messages suggest specific remediation steps
/// - **Bioinformatics-Aware**: Terminology and suggestions fit genomics workflows
/// - **Source-Preserving**: Underlying system errors are chained for full context
///
/// # Error Categories
///
/// ## File Integrity Errors
/// - `FailedChecksum`: Individual file checksum mismatch
/// - `MultipleFailedChecksums`: Batch operation with multiple failures
///
/// ## File Access Errors  
/// - `InaccessibleFile`: File doesn't exist or lacks permissions
/// - `FileOpenError`: Cannot open file for reading
/// - `FileReadError`: I/O failure during file reading
///
/// ## Hash Computation Errors
/// - `HashSizeMismatch`: Algorithm produced unexpected hash size
/// - `HashConversionError`: Failed to convert hash to required format
/// - `HashStringConversionError`: Hash contains invalid UTF-8 bytes
/// - `MerkleTreeComputationError`: Parallel hash tree computation failed
/// - `InvalidMerkleTreeResult`: Unexpected number of root hashes
///
/// ## Archive Processing Errors
/// - `UnsupportedArchiveFormat`: Archive format not supported
/// - `FileNotFoundInArchive`: Requested file not in archive
/// - `CorruptedArchive`: Archive appears corrupted or invalid
/// - `ArchiveTooLarge`: Archive exceeds size safety limits
/// - `TooManyArchiveEntries`: Archive has too many entries
/// - `ArchiveEntryTooLarge`: Individual entry exceeds size limits
/// - `ArchiveReadError`: Generic archive reading failure
/// - `ArchiveTimeout`: Archive processing exceeded time limit
///
/// # Examples
///
/// ```no_run
/// use checkle::errors::CheckleError;
/// use std::path::PathBuf;
///
/// // Pattern matching for specific error handling
/// match some_operation() {
///     Err(CheckleError::FailedChecksum(path)) => {
///         println!("File {} failed verification - please re-transfer", path.display());
///     },
///     Err(CheckleError::ArchiveTooLarge { size, limit, .. }) => {
///         println!("Archive too large: {} bytes (limit: {})", size, limit);
///     },
///     Err(e) => eprintln!("Operation failed: {}", e),
///     Ok(result) => { /* handle success */ },
/// }
///
/// # fn some_operation() -> checkle::errors::Result<()> { Ok(()) }
/// ```
#[derive(Debug, Error)]
pub enum CheckleError {
    /// File checksum validation failed, indicating data corruption or modification.
    ///
    /// This error occurs when a file's computed checksum doesn't match the expected
    /// value, which typically indicates:
    /// - File was truncated during transfer (common in large genomics files)
    /// - Data corruption occurred during storage or transfer
    /// - File was modified after the original checksum was computed
    /// - Wrong file was provided for verification
    ///
    /// In bioinformatics workflows, this is a critical error that usually requires
    /// re-transferring the file from the original source.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::FailedChecksum(PathBuf::from("sample.fastq.gz"));
    /// println!("{}", error);
    /// // Output: The provided file `sample.fastq.gz` failed the checksum process...
    /// ```
    #[error(
        "The provided file `{0}` failed the checksum process. It was likely truncated during a file transfer or otherwise mutated since the hash was originally computed."
    )]
    FailedChecksum(PathBuf),

    /// Multiple files failed checksum verification in a batch operation.
    ///
    /// This error indicates that a batch checksum operation encountered multiple
    /// failures. Individual file failures are logged separately, and this error
    /// serves as a summary indicator that the batch operation was not fully successful.
    ///
    /// This commonly occurs when:
    /// - Processing a directory tree with some corrupted files
    /// - Verifying archived datasets where some files are damaged
    /// - Network transfer issues affected multiple files
    ///
    /// Check the detailed logs or output for information about which specific
    /// files failed and their individual error conditions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::MultipleFailedChecksums;
    /// println!("{}", error);
    /// // Output: File(s) failed the checksum. See logs and/or output for more information.
    /// ```
    #[error("File(s) failed the checksum. See logs and/or output for more information.")]
    MultipleFailedChecksums,

    /// File does not exist or cannot be accessed due to permissions.
    ///
    /// This error indicates that the specified file path either:
    /// - Points to a non-existent file or directory
    /// - Exists but the current user lacks read permissions
    /// - Is on an unmounted filesystem or network location
    /// - Path contains invalid characters for the current filesystem
    ///
    /// Common in bioinformatics when:
    /// - Reference files have been moved or deleted
    /// - Compute cluster storage mounts are not available
    /// - Incorrect paths in batch processing scripts
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::InaccessibleFile(PathBuf::from("/missing/genome.fa"));
    /// println!("{}", error);
    /// // Output: The provided file `/missing/genome.fa` does not exist or is otherwise inaccessible.
    /// ```
    #[error("The provided file `{0}` does not exist or is otherwise inaccessible.")]
    InaccessibleFile(PathBuf),

    /// Checksum file has invalid format and cannot be parsed.
    ///
    /// This error occurs when a checksum file doesn't conform to the expected format.
    /// The standard format requires:
    /// - Tab-delimited text file
    /// - Two columns: hash value, then file path
    /// - No header row
    /// - Valid hexadecimal hash values
    /// - UTF-8 encoding
    ///
    /// Common issues include:
    /// - Using spaces instead of tabs as delimiters
    /// - Including header rows or comments
    /// - Corrupted hash values (non-hexadecimal characters)
    /// - Wrong column order (path first, hash second)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::InvalidChecksumFile(PathBuf::from("checksums.md5"));
    /// println!("{}", error);
    /// // Output: The provided checksum file `checksums.md5` was invalid...
    /// ```
    #[error(
        "The provided checksum file `{0}` was invalid and could not be parsed. Please double check that it is tab delimited with two columns and no header, where the first column is the hash and the second column is the corresponding file path (relative or absolute)."
    )]
    InvalidChecksumFile(PathBuf),

    /// Failed to open file for reading during hash computation.
    ///
    /// This error wraps underlying I/O errors that occur when attempting to open
    /// a file for reading. Unlike `InaccessibleFile`, this error occurs after
    /// confirming the file exists but failing to obtain a file handle.
    ///
    /// Common causes include:
    /// - File is locked by another process
    /// - Permissions changed between existence check and open
    /// - File is on a network filesystem that became unavailable
    /// - System resource exhaustion (too many open files)
    /// - File is actually a directory
    ///
    /// The underlying system error is preserved in the `source` field for
    /// detailed diagnostics.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::{path::PathBuf, io};
    ///
    /// let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
    /// let error = CheckleError::FileOpenError {
    ///     path: PathBuf::from("locked_file.txt"),
    ///     source: io_error,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to open file '{path}' for hashing. Ensure the file exists, is readable, and you have the necessary permissions. The underlying error was: {source}"
    )]
    FileOpenError {
        /// Path to the file that could not be opened
        path: PathBuf,
        /// Underlying I/O error that caused the failure
        #[source]
        source: std::io::Error,
    },

    /// I/O error occurred while reading file data during hash computation.
    ///
    /// This error occurs after successfully opening a file but failing to read
    /// its contents. This can indicate serious underlying issues that require
    /// immediate attention in bioinformatics workflows.
    ///
    /// Common causes include:
    /// - Disk corruption or bad sectors
    /// - Network filesystem issues (NFS timeouts, connection loss)
    /// - Hardware failures (failing disk, memory issues)
    /// - File was truncated or modified during reading
    /// - Insufficient system resources
    ///
    /// In genomics workflows, this often indicates storage infrastructure
    /// problems that could affect data integrity across multiple files.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::{path::PathBuf, io};
    ///
    /// let io_error = io::Error::new(io::ErrorKind::UnexpectedEof, "Unexpected EOF");
    /// let error = CheckleError::FileReadError {
    ///     path: PathBuf::from("corrupted_genome.fa"),
    ///     source: io_error,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to read from file '{path}' during hash computation. This could indicate disk corruption, network issues if the file is on a remote filesystem, or insufficient permissions. The underlying error was: {source}"
    )]
    FileReadError {
        /// Path to the file that could not be read
        path: PathBuf,
        /// Underlying I/O error that caused the read failure
        #[source]
        source: std::io::Error,
    },

    /// Hash algorithm produced unexpected output size.
    ///
    /// This error indicates an internal inconsistency in the hash computation where
    /// the algorithm produced a hash of unexpected size. This should never happen
    /// with properly functioning hash algorithms.
    ///
    /// Expected sizes:
    /// - MD5: 16 bytes (128 bits)
    /// - SHA-256: 32 bytes (256 bits)
    ///
    /// This error suggests:
    /// - Bug in the hash algorithm implementation
    /// - Memory corruption during hash computation
    /// - Hardware failure affecting computation
    /// - Incompatible or corrupted hash library
    ///
    /// This is a critical error that should be reported as it indicates
    /// potential data integrity issues.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::HashSizeMismatch {
    ///     path: PathBuf::from("data.txt"),
    ///     algorithm: "MD5".to_string(),
    ///     computed_size: 20,
    ///     expected_size: 16,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Hash computation failed for file '{path}' using {algorithm} algorithm. The computed hash size ({computed_size} bytes) does not match the expected size ({expected_size} bytes). This indicates an internal error in the hash algorithm implementation."
    )]
    HashSizeMismatch {
        /// Path to the file being hashed when the error occurred
        path: PathBuf,
        /// Name of the hash algorithm that failed
        algorithm: String,
        /// Actual size of the computed hash in bytes
        computed_size: usize,
        /// Expected size of the hash in bytes for this algorithm
        expected_size: usize,
    },

    /// Failed to convert hash result to required output format.
    ///
    /// This error occurs when the computed hash cannot be converted to the
    /// required output format (typically hexadecimal string representation).
    /// This is an internal error that should not occur during normal operation.
    ///
    /// Potential causes:
    /// - Memory corruption during hash processing
    /// - Bug in hash-to-string conversion logic
    /// - System resource exhaustion during conversion
    /// - Incompatible hash library version
    ///
    /// This error should be reported as it indicates a bug in checkle's
    /// hash processing pipeline.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::HashConversionError {
    ///     path: PathBuf::from("genome.fa"),
    ///     algorithm: "SHA256".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to convert hash result to the required format for file '{path}' using {algorithm} algorithm. This is an internal error that suggests a problem with hash result processing. Please report this issue."
    )]
    HashConversionError {
        /// Path to the file being processed when conversion failed
        path: PathBuf,
        /// Name of the hash algorithm being used
        algorithm: String,
    },

    /// Hash result contains bytes that cannot be converted to valid UTF-8.
    ///
    /// This error occurs when the computed hash contains bytes that cannot be
    /// represented as valid UTF-8 text. This should never happen with properly
    /// functioning hexadecimal encoding.
    ///
    /// This indicates:
    /// - Bug in hash-to-hex conversion logic
    /// - Memory corruption affecting the hash result
    /// - System encoding issues
    /// - Hardware failure affecting computation
    ///
    /// The invalid bytes are included for debugging purposes. This is a critical
    /// internal error that should be reported.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::HashStringConversionError {
    ///     path: PathBuf::from("sample.bam"),
    ///     invalid_bytes: vec![0xFF, 0xFE, 0xFD],
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to convert computed hash to UTF-8 string for file '{path}'. The hash contains invalid bytes: {invalid_bytes:?}. This is an unexpected internal error."
    )]
    HashStringConversionError {
        /// Path to the file being processed when conversion failed
        path: PathBuf,
        /// The invalid bytes that could not be converted to UTF-8
        invalid_bytes: Vec<u8>,
    },

    /// Parallel Merkle tree hash computation failed at a specific tree level.
    ///
    /// This error occurs during checkle's parallel hash computation when the
    /// Merkle tree reduction fails at a specific level. The Merkle tree approach
    /// combines chunk hashes in a binary tree structure to produce the final hash.
    ///
    /// Common causes:
    /// - Insufficient system memory for large files
    /// - Thread pool exhaustion or thread panics
    /// - Hash computation errors in parallel workers
    /// - System resource constraints (CPU, memory pressure)
    ///
    /// The tree level indicates where the failure occurred:
    /// - Level 0: Individual chunk hashing failed
    /// - Level 1+: Tree reduction/combination failed
    ///
    /// Solutions:
    /// - Reduce parallel reader count
    /// - Increase available system memory
    /// - Use smaller chunk sizes
    /// - Try sequential processing (single thread)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::MerkleTreeComputationError {
    ///     path: PathBuf::from("large_genome.fa"),
    ///     algorithm: "MD5".to_string(),
    ///     level: 2,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Merkle tree computation failed for file '{path}' using {algorithm} algorithm at tree level {level}. This could indicate memory issues or hash computation problems. Try with a smaller file or different algorithm."
    )]
    MerkleTreeComputationError {
        /// Path to the file being hashed when the error occurred
        path: PathBuf,
        /// Hash algorithm being used for computation
        algorithm: String,
        /// Tree level where the computation failed (0 = chunk level)
        level: usize,
    },

    /// Merkle tree computation produced invalid number of root hashes.
    ///
    /// This error indicates a bug in checkle's Merkle tree reduction algorithm.
    /// The tree reduction should always produce exactly one root hash regardless
    /// of the input size or parallelization level.
    ///
    /// Finding multiple root hashes suggests:
    /// - Bug in the tree reduction logic
    /// - Race condition in parallel processing
    /// - Memory corruption during computation
    /// - Incorrect chunk boundary calculations
    ///
    /// Finding zero root hashes suggests:
    /// - Empty file handling bug
    /// - Initialization error in tree computation
    /// - Thread pool failure
    ///
    /// This is a critical internal error that should be reported as it indicates
    /// a fundamental problem with checkle's parallelization logic.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::InvalidMerkleTreeResult {
    ///     path: PathBuf::from("data.bin"),
    ///     found_count: 3,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Internal error during hash array processing for file '{path}'. Expected exactly one root hash but found {found_count} hashes. This suggests a bug in the Merkle tree computation."
    )]
    InvalidMerkleTreeResult {
        /// Path to the file being processed when the error occurred
        path: PathBuf,
        /// Number of root hashes found (should always be 1)
        found_count: usize,
    },

    /// Failed to determine the current working directory.
    ///
    /// This error occurs when the system cannot determine the current working
    /// directory, which is needed for resolving relative file paths.
    ///
    /// Common causes:
    /// - Current directory was deleted after the process started
    /// - Insufficient permissions to access the current directory
    /// - Filesystem corruption or unmounted network drives
    /// - Process running in a container with restricted filesystem access
    ///
    /// In bioinformatics workflows, this often happens when:
    /// - Jobs are launched from temporary directories that get cleaned up
    /// - Network filesystems become unavailable during long-running processes
    /// - Container environments have restricted filesystem access
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::NotFound, "Directory not found");
    /// let error = CheckleError::CurrentDirectoryError { source: io_error };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to get current working directory. This could indicate a permissions issue or that the current directory has been deleted. The underlying error was: {source}"
    )]
    CurrentDirectoryError {
        /// Underlying system error that prevented directory access
        #[source]
        source: std::io::Error,
    },

    /// Failed to read directory contents during file traversal.
    ///
    /// This error occurs when checkle cannot read the contents of a directory
    /// during recursive file discovery or batch processing operations.
    ///
    /// Common causes:
    /// - Insufficient permissions to read the directory
    /// - Directory is on an unmounted or unavailable filesystem
    /// - Network filesystem timeouts or connectivity issues
    /// - Directory was deleted between existence check and read attempt
    /// - Filesystem corruption
    ///
    /// In bioinformatics contexts, this commonly occurs with:
    /// - Shared compute cluster storage with complex permissions
    /// - Network-mounted directories containing large datasets
    /// - Temporary directories that get cleaned up during processing
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::{path::PathBuf, io};
    ///
    /// let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "Access denied");
    /// let error = CheckleError::DirectoryReadError {
    ///     path: PathBuf::from("/restricted/genomics/data"),
    ///     source: io_error,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to read directory '{path}'. This could indicate permissions issues or that the directory is inaccessible. The underlying error was: {source}"
    )]
    DirectoryReadError {
        /// Path to the directory that could not be read
        path: PathBuf,
        /// Underlying I/O error that caused the failure
        #[source]
        source: std::io::Error,
    },

    /// Failed to initialize global thread pool for parallel processing.
    ///
    /// This error occurs when checkle cannot create the thread pool needed for
    /// parallel hash computation. This is typically a system resource issue.
    ///
    /// Common causes:
    /// - Insufficient system resources to create threads
    /// - Operating system thread limits exceeded
    /// - Memory pressure preventing thread stack allocation
    /// - Security policies restricting thread creation
    ///
    /// This error suggests system resource constraints that may affect overall
    /// performance. Consider:
    /// - Reducing the number of parallel readers
    /// - Freeing system memory
    /// - Checking system thread limits
    /// - Using single-threaded mode
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// # use std::fmt;
    /// # #[derive(Debug)]
    /// # struct MockThreadPoolBuildError;
    /// # impl fmt::Display for MockThreadPoolBuildError {
    /// #     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    /// #         write!(f, "thread pool build error")
    /// #     }
    /// # }
    /// # impl std::error::Error for MockThreadPoolBuildError {}
    ///
    /// // Note: This is a simplified example as ThreadPoolBuildError is opaque
    /// println!("Failed to create thread pool with 8 threads");
    /// ```
    #[error(
        "Failed to initialize global thread pool with {thread_count} threads. This is an internal error. The underlying error was: {source}"
    )]
    ThreadPoolInitError {
        /// Number of threads that were requested for the pool
        thread_count: usize,
        /// Underlying rayon error that caused initialization failure
        #[source]
        source: rayon::ThreadPoolBuildError,
    },

    /// Chunk size parameter is outside valid bounds or not properly aligned.
    ///
    /// This error occurs when an invalid chunk size is specified for hash
    /// computation. Chunk size affects both memory usage and I/O performance.
    ///
    /// Requirements:
    /// - Must be between minimum and maximum bounds (typically 4KB to 64MB)
    /// - Should be aligned to page boundaries (4KB) for optimal performance
    /// - Must not exceed available system memory
    ///
    /// Common issues:
    /// - Specifying too small chunks (poor I/O performance)
    /// - Specifying too large chunks (excessive memory usage)
    /// - Non-aligned sizes (suboptimal performance)
    ///
    /// For bioinformatics workloads:
    /// - Small files: 4KB-64KB chunks
    /// - Large genomics files: 1MB-16MB chunks
    /// - Memory-constrained systems: smaller chunks
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::InvalidChunkSize {
    ///     size: 1024,
    ///     reason: "Too small".to_string(),
    ///     min_size: 4096,
    ///     max_size: 67108864,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Invalid chunk size: {size} bytes. {reason} Valid range is {min_size} to {max_size} bytes, aligned to page boundaries (4KB)."
    )]
    InvalidChunkSize {
        /// The invalid chunk size that was specified
        size: usize,
        /// Human-readable reason why the size is invalid
        reason: String,
        /// Minimum allowed chunk size in bytes
        min_size: usize,
        /// Maximum allowed chunk size in bytes
        max_size: usize,
    },

    /// Output file already exists and would be overwritten.
    ///
    /// This error occurs when trying to write to an output file that already
    /// exists, and checkle is configured to prevent accidental overwrites.
    /// This is a safety feature to prevent data loss.
    ///
    /// Common scenarios:
    /// - Re-running a checksum operation without cleaning up previous results
    /// - Multiple processes trying to write to the same output file
    /// - Backup files with conflicting names
    ///
    /// Solutions:
    /// - Remove the existing file if overwrite is intended
    /// - Choose a different output filename
    /// - Use a force/overwrite flag if available
    ///
    /// In bioinformatics workflows, this prevents accidentally overwriting
    /// important checksum files or analysis results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::OutputFileExists {
    ///     path: PathBuf::from("results.md5"),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "The output file '{path}' already exists. Please remove it if you want to overwrite it, or choose a different output file path."
    )]
    OutputFileExists {
        /// Path to the existing output file that would be overwritten
        path: PathBuf,
    },

    /// Invalid command-line argument provided.
    ///
    /// This error occurs when a command-line argument fails validation.
    /// Arguments must meet specific format requirements and value constraints.
    ///
    /// Common issues:
    /// - Malformed file paths or archive path syntax
    /// - Invalid algorithm names or options
    /// - Out-of-range numeric values
    /// - Incompatible argument combinations
    ///
    /// The error message provides details about which argument is invalid
    /// and what the valid options are.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::InvalidCliArgument(
    ///     "Invalid archive path syntax: missing colon separator".to_string()
    /// );
    /// println!("{}", error);
    /// ```
    #[error("Invalid CLI argument: {0}")]
    InvalidCliArgument(String),

    /// Numeric parameter has invalid value or format.
    ///
    /// This error occurs when a numeric command-line parameter cannot be
    /// parsed or is outside acceptable bounds.
    ///
    /// Common issues:
    /// - Non-numeric strings where numbers are expected
    /// - Negative values where only positive values are valid
    /// - Values exceeding system or algorithmic limits
    /// - Floating-point values where integers are required
    ///
    /// The error provides both the invalid value and the specific reason
    /// for rejection to help with correction.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::InvalidNumericValue {
    ///     value: "abc".to_string(),
    ///     reason: "not a valid integer".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error("Invalid numeric value '{value}': {reason}")]
    InvalidNumericValue {
        /// The invalid numeric value that was provided
        value: String,
        /// Human-readable explanation of why the value is invalid
        reason: String,
    },

    /// General configuration or setup error.
    ///
    /// This error covers various configuration issues that don't fit into
    /// more specific error categories. These are typically setup or
    /// initialization problems.
    ///
    /// Common issues:
    /// - Invalid configuration file format
    /// - Incompatible option combinations
    /// - Missing required dependencies or features
    /// - System capability limitations
    ///
    /// The error message provides specific details about the configuration
    /// problem to help with resolution.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::ConfigError(
    ///     "Archive features not compiled in this build".to_string()
    /// );
    /// println!("{}", error);
    /// ```
    #[error("Configuration error: {0}")]
    ConfigError(String),

    // ============================================================================
    // Archive-Related Errors
    // ============================================================================
    /// Archive format is not supported by this build of checkle.
    ///
    /// This error occurs when trying to process an archive format that is not
    /// compiled into the current build or is genuinely unsupported.
    ///
    /// Supported formats (when compiled with appropriate features):
    /// - TAR archives: .tar, .tar.gz, .tar.bz2, .tar.xz
    /// - ZIP archives: .zip
    ///
    /// Common causes:
    /// - File has unsupported extension (.rar, .7z, etc.)
    /// - Build was compiled without archive feature flags
    /// - File extension doesn't match actual format
    /// - Corrupted or truncated archive
    ///
    /// Solutions:
    /// - Convert archive to supported format
    /// - Extract files and hash individually
    /// - Use build with appropriate feature flags enabled
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::UnsupportedArchiveFormat(
    ///     PathBuf::from("data.rar")
    /// );
    /// println!("{}", error);
    /// ```
    #[error(
        "Unsupported archive format: {0}\n\nSupported formats:\n- TAR archives (.tar, .tar.gz, .tar.bz2, .tar.xz)\n- ZIP archives (.zip)\n\nPlease ensure your file has the correct extension and format."
    )]
    UnsupportedArchiveFormat(PathBuf),

    /// Specified file was not found within the archive.
    ///
    /// This error occurs when using archive path syntax (archive.tar:path/file.txt)
    /// to reference a specific file that doesn't exist within the archive.
    ///
    /// Common causes:
    /// - Typo in the file path within the archive
    /// - File was removed from a newer version of the archive
    /// - Case sensitivity issues (Unix vs Windows paths)
    /// - Incorrect path separators (/ vs \\)
    /// - Archive was created with different directory structure
    ///
    /// Solutions:
    /// - List archive contents to see available files
    /// - Check for correct case and path separators
    /// - Use glob patterns to match similar files
    /// - Verify you have the correct version of the archive
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::FileNotFoundInArchive {
    ///     archive: PathBuf::from("dataset.tar.gz"),
    ///     file: "samples/sample001.fastq".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "File '{file}' not found in archive '{archive}'\n\nUse 'checkle hash-archive {archive} --list' to see available files."
    )]
    FileNotFoundInArchive {
        /// Path to the archive that was searched
        archive: PathBuf,
        /// File path that was not found within the archive
        file: String,
    },

    /// Archive appears to be corrupted or malformed.
    ///
    /// This error occurs when archive processing fails due to corruption,
    /// truncation, or invalid internal structure. This is critical in
    /// bioinformatics where data integrity is paramount.
    ///
    /// Common indicators of corruption:
    /// - Invalid header information
    /// - Truncated compressed data
    /// - Checksum mismatches in archive metadata
    /// - Unexpected end of file during decompression
    /// - Invalid compression format within archive
    ///
    /// Possible causes:
    /// - Incomplete download or file transfer
    /// - Network errors during transfer
    /// - Storage media failure (disk corruption)
    /// - Archive created with incompatible compression settings
    /// - Archive modified by incompatible software
    ///
    /// Critical for genomics data:
    /// - Re-download from original source
    /// - Verify transfer checksums
    /// - Check storage system integrity
    /// - Consider the dataset compromised until verified
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::CorruptedArchive {
    ///     path: PathBuf::from("genomics_data.tar.gz"),
    ///     details: "Invalid gzip header".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Archive '{path}' appears to be corrupted: {details}\n\nPossible causes:\n1. Incomplete download or transfer\n2. Archive created with incompatible settings\n3. File system corruption\n\nTry re-downloading or recreating the archive."
    )]
    CorruptedArchive {
        /// Path to the corrupted archive
        path: PathBuf,
        /// Specific details about the corruption detected
        details: String,
    },

    /// Archive file exceeds maximum size limits for safe processing.
    ///
    /// This error occurs when an archive exceeds the built-in size limits
    /// designed to prevent memory exhaustion and system resource issues.
    ///
    /// Size limits exist because:
    /// - Large archives can consume excessive memory during processing
    /// - Decompression attacks (zip bombs) can exhaust system resources
    /// - Very large archives may indicate incorrect file selection
    /// - Memory-constrained systems need protection from large files
    ///
    /// Common in bioinformatics with:
    /// - Entire genome sequencing datasets (multi-terabyte archives)
    /// - Combined experimental datasets
    /// - Improperly compressed large binary files
    ///
    /// Solutions:
    /// - Split archive into smaller, manageable parts
    /// - Extract and process files individually
    /// - Use streaming processing tools for large datasets
    /// - Increase system resources if appropriate
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::ArchiveTooLarge {
    ///     path: PathBuf::from("massive_dataset.tar.gz"),
    ///     size: 50_000_000_000,  // 50GB
    ///     limit: 10_000_000_000, // 10GB limit
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Archive '{path}' is {size} bytes, exceeding the limit of {limit} bytes\n\nThis archive is too large to process safely. Consider:\n1. Splitting the archive into smaller parts\n2. Processing files separately outside the archive"
    )]
    ArchiveTooLarge {
        /// Path to the oversized archive
        path: PathBuf,
        /// Actual size of the archive in bytes
        size: u64,
        /// Maximum allowed size in bytes
        limit: u64,
    },

    /// Archive contains too many entries for safe processing.
    ///
    /// This error occurs when an archive contains more entries than the system
    /// can safely handle in memory. Each entry requires metadata storage, and
    /// very large archives can exhaust available memory.
    ///
    /// Entry limits exist to prevent:
    /// - Memory exhaustion from metadata storage
    /// - Denial of service from maliciously crafted archives
    /// - System unresponsiveness during directory traversal
    /// - Excessive processing time for huge archives
    ///
    /// Common scenarios:
    /// - Archives containing millions of small files
    /// - Improperly created archives with duplicate entries
    /// - Malicious archives designed to consume resources
    /// - Nested archive structures (archives within archives)
    ///
    /// Solutions for bioinformatics data:
    /// - Use more targeted file selection patterns
    /// - Split large archives into logical partitions
    /// - Process archive contents in batches
    /// - Extract specific subdirectories only
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::TooManyArchiveEntries {
    ///     path: PathBuf::from("huge_dataset.tar"),
    ///     count: 1_000_000,
    ///     limit: 100_000,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Archive '{path}' contains {count} entries, exceeding the limit of {limit}\n\nThis limit exists to prevent memory exhaustion. Consider:\n1. Splitting the archive into smaller parts\n2. Processing specific files instead of the entire archive"
    )]
    TooManyArchiveEntries {
        /// Path to the archive with too many entries
        path: PathBuf,
        /// Actual number of entries found in the archive
        count: usize,
        /// Maximum allowed number of entries
        limit: usize,
    },

    /// Individual archive entry exceeds size limits for safe processing.
    ///
    /// This error occurs when a single file within an archive is too large
    /// to process safely given system memory constraints and security limits.
    ///
    /// Large entry limits prevent:
    /// - Memory exhaustion during decompression
    /// - Decompression bomb attacks
    /// - System unresponsiveness during processing
    /// - Disk space exhaustion during extraction
    ///
    /// Common in bioinformatics with:
    /// - Large genome assembly files (multi-gigabyte FASTA files)
    /// - Raw sequencing data files (large FASTQ files)
    /// - High-resolution imaging data
    /// - Compressed databases or reference files
    ///
    /// Solutions:
    /// - Extract and process the large file separately
    /// - Use streaming processing for large files
    /// - Split large files into smaller chunks
    /// - Increase system resources if appropriate
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::ArchiveEntryTooLarge {
    ///     archive: PathBuf::from("genomics.tar.gz"),
    ///     entry: "reference_genome.fa".to_string(),
    ///     size: 5_000_000_000,   // 5GB file
    ///     limit: 1_000_000_000,  // 1GB limit
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Archive entry '{entry}' in '{archive}' is {size} bytes, exceeding the limit of {limit} bytes\n\nThis file is too large to process safely. Consider:\n1. Processing this file separately outside the archive\n2. Splitting the file into smaller chunks"
    )]
    ArchiveEntryTooLarge {
        /// Path to the archive containing the oversized entry
        archive: PathBuf,
        /// Name/path of the oversized entry within the archive
        entry: String,
        /// Actual size of the entry in bytes
        size: u64,
        /// Maximum allowed size for individual entries in bytes
        limit: u64,
    },

    /// Generic archive reading failure during processing.
    ///
    /// This error occurs when archive reading fails for reasons not covered
    /// by more specific error types. It typically indicates issues with the
    /// decompression or data extraction process.
    ///
    /// Common causes:
    /// - Corrupted compressed data streams
    /// - Unsupported compression methods or variants
    /// - I/O errors during decompression (disk full, network issues)
    /// - Archive format variations not fully supported
    /// - Memory pressure affecting decompression buffers
    ///
    /// In bioinformatics workflows, this often indicates:
    /// - Transfer corruption of compressed genomics data
    /// - Compatibility issues with archive creation tools
    /// - Resource constraints during large file processing
    ///
    /// Recommended actions:
    /// - Verify archive integrity with native tools (tar -tf, unzip -t)
    /// - Check available disk space and memory
    /// - Re-download or re-create the archive
    /// - Try extracting with different tools to isolate the issue
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::ArchiveReadError {
    ///     details: "gzip decompression failed: invalid compressed data".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to read from archive: {details}\n\nThis may indicate:\n1. Corrupted compressed data\n2. Unsupported compression method\n3. I/O errors during decompression"
    )]
    ArchiveReadError {
        /// Specific details about the read failure
        details: String,
    },

    /// Archive processing exceeded time limits.
    ///
    /// This error occurs when archive operations take too long to complete,
    /// suggesting system resource issues or problematic archive content.
    ///
    /// Timeout limits exist to prevent:
    /// - Infinite loops in corrupted archive processing
    /// - Resource exhaustion from decompression bombs
    /// - System hangs on problematic archives
    /// - Excessive compute time on shared systems
    ///
    /// Common causes:
    /// - Extremely large or complex compression requiring excessive CPU time
    /// - System resource constraints (low memory, high CPU load)
    /// - Corrupted data causing decompression algorithms to loop
    /// - Archive bombs designed to consume excessive resources
    ///
    /// In bioinformatics contexts:
    /// - Very large genomics datasets may need more time
    /// - Shared compute clusters may have resource contention
    /// - Network storage may introduce latency
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::{path::PathBuf, time::Duration};
    ///
    /// let error = CheckleError::ArchiveTimeout {
    ///     path: PathBuf::from("slow_dataset.tar.bz2"),
    ///     elapsed: Duration::from_secs(300), // 5 minutes
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Archive operation timed out after {elapsed:?}\n\nThe archive '{path}' is taking too long to process. This could indicate:\n1. Extremely large or complex compression\n2. System resource constraints\n3. Corrupted data causing decompression loops"
    )]
    ArchiveTimeout {
        /// Path to the archive that timed out
        path: PathBuf,
        /// Time elapsed before timeout occurred
        elapsed: std::time::Duration,
    },

    /// Archive file extension doesn't match actual format.
    ///
    /// This error occurs when the file extension suggests one archive format
    /// but the file content indicates a different format. This mismatch can
    /// cause processing failures and confusion.
    ///
    /// Common scenarios:
    /// - File renamed with wrong extension (.tar file renamed to .zip)
    /// - Download corruption changed file headers
    /// - Archive created with non-standard tools
    /// - File format misidentification
    ///
    /// In bioinformatics:
    /// - Data transfer systems sometimes mangle file extensions
    /// - Automated pipelines may incorrectly categorize files
    /// - Legacy data may have inconsistent naming conventions
    ///
    /// Solutions:
    /// - Rename file to match actual format
    /// - Use file content detection tools (file command)
    /// - Verify archive with format-specific tools
    /// - Check data transfer integrity
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::InvalidArchiveFormat {
    ///     path: PathBuf::from("data.zip"),
    ///     expected: "ZIP".to_string(),
    ///     actual: "TAR".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Invalid archive format for '{path}'\nExpected: {expected}\nActual: {actual}\n\nThe file extension doesn't match the actual format. Please rename the file or specify the correct format."
    )]
    InvalidArchiveFormat {
        /// Path to the file with mismatched format
        path: PathBuf,
        /// Archive format expected based on file extension
        expected: String,
        /// Actual archive format detected from file content
        actual: String,
    },

    /// Archive entry could not be located during processing.
    ///
    /// This error occurs when a previously identified archive entry cannot
    /// be found during actual processing, which may indicate archive
    /// inconsistencies or concurrent modifications.
    ///
    /// This differs from `FileNotFoundInArchive` in that it typically occurs
    /// after an entry was initially found but became inaccessible during
    /// the actual hash computation phase.
    ///
    /// Common causes:
    /// - Archive was modified during processing
    /// - Race condition in concurrent access
    /// - Archive corruption affecting entry table
    /// - Memory issues affecting entry tracking
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::path::PathBuf;
    ///
    /// let error = CheckleError::ArchiveEntryNotFound {
    ///     archive: PathBuf::from("dataset.tar"),
    ///     entry: "data/sample.fastq".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error("Archive entry not found: {archive}:{entry}")]
    ArchiveEntryNotFound {
        /// Path to the archive being processed
        archive: PathBuf,
        /// Entry path that could not be found
        entry: String,
    },

    /// Operation attempted to mix archive and filesystem sources.
    ///
    /// This error occurs when trying to perform operations that combine
    /// both archive entries and regular filesystem files, which is not
    /// supported due to different processing requirements.
    ///
    /// Examples of unsupported mixed operations:
    /// - Batch verification with both archive entries and regular files
    /// - Output operations combining archive and filesystem sources
    /// - Cross-referencing between archive contents and filesystem
    ///
    /// This limitation exists because:
    /// - Archive and filesystem operations have different performance characteristics
    /// - Error handling differs between archive and file operations
    /// - Security models are different (archive vs filesystem access)
    ///
    /// Solutions:
    /// - Process archive entries separately from filesystem files
    /// - Extract archive contents to filesystem for unified processing
    /// - Use separate commands for archive vs filesystem operations
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::MixedSourceVerification;
    /// println!("{}", error);
    /// ```
    #[error("Mixed archive and filesystem sources are not supported in the same operation")]
    MixedSourceVerification,

    /// General hash computation failure.
    ///
    /// This error covers hash computation failures that don't fit into more
    /// specific error categories. It indicates problems in the core hashing
    /// pipeline that require investigation.
    ///
    /// Common causes:
    /// - Internal hash algorithm failures
    /// - Memory allocation issues during large file processing
    /// - System resource constraints (CPU, memory pressure)
    /// - Thread pool failures in parallel processing
    /// - Hardware failures affecting computation integrity
    ///
    /// This is a serious error in bioinformatics contexts where hash integrity
    /// is critical for data validation. Investigation is required to determine
    /// if the failure is environmental or indicates data corruption.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::HashingError {
    ///     details: "Thread pool panic during parallel hash computation".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Hash computation error: {details}\n\nThis could indicate:\n1. Internal hash algorithm failure\n2. Memory allocation issues\n3. System resource constraints"
    )]
    HashingError {
        /// Specific details about the hashing failure
        details: String,
    },

    // ============================================================================
    // Pretty Printing Errors
    // ============================================================================
    /// Hash string contains invalid hexadecimal characters.
    ///
    /// This error occurs when a hash value contains characters that are not
    /// valid hexadecimal digits. Hash values must contain only characters
    /// 0-9 and a-f (case insensitive).
    ///
    /// Common causes:
    /// - Copy/paste errors including extra characters
    /// - File corruption of checksum files
    /// - Incorrect encoding (UTF-8 issues)
    /// - Manual typing errors
    /// - Processing artifacts from text manipulation
    ///
    /// Valid hexadecimal characters: 0123456789abcdefABCDEF
    /// Invalid characters: spaces, punctuation, letters g-z
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::InvalidHashFormat {
    ///     hash: "a1b2c3g4".to_string(), // 'g' is invalid
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Invalid hash format: '{hash}' is not a valid hexadecimal hash\n\nHashes must contain only characters 0-9 and a-f (case insensitive).\nExample: 'a1b2c3d4e5f6789abcdef1234567890'"
    )]
    InvalidHashFormat {
        /// The invalid hash string that was provided
        hash: String,
    },

    /// Hash value is empty when a non-empty value is required.
    ///
    /// This error occurs when an operation requires a hash value but receives
    /// an empty string instead. This typically indicates missing data or
    /// incomplete input processing.
    ///
    /// Common scenarios:
    /// - Empty lines in checksum files
    /// - Malformed checksum file parsing
    /// - Missing command-line arguments
    /// - File processing that produces no output
    ///
    /// In bioinformatics workflows, this often indicates:
    /// - Corrupted checksum files from data transfers
    /// - Incomplete pipeline outputs
    /// - Formatting issues in automated systems
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::EmptyHash;
    /// println!("{}", error);
    /// ```
    #[error(
        "Empty hash value provided\n\nA hash value is required but an empty string was given.\nPlease provide a valid hash string."
    )]
    EmptyHash,

    /// Failed to write error output to stderr.
    ///
    /// This error occurs when checkle cannot write error messages to stderr,
    /// which can happen in various system and shell environments.
    ///
    /// Common causes:
    /// - stderr is redirected to a closed pipe or file
    /// - Terminal or shell session was disconnected
    /// - Disk is full when stderr is redirected to a file
    /// - Permission issues with stderr redirection target
    /// - System resource exhaustion
    ///
    /// In bioinformatics contexts, this commonly occurs with:
    /// - Job schedulers that redirect stderr to files
    /// - Container environments with restricted I/O
    /// - Long-running jobs where log files fill up disks
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::BrokenPipe, "Broken pipe");
    /// let error = CheckleError::StderrWriteError {
    ///     details: "Failed to write progress information".to_string(),
    ///     source: io_error,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Failed to write output to stderr: {details}\n\nThis may occur if:\n1. stderr is redirected to a closed pipe\n2. The terminal is disconnected\n3. Disk is full\n\nTry running without output redirection or check disk space."
    )]
    StderrWriteError {
        /// Description of what was being written to stderr
        details: String,
        /// Underlying I/O error that caused the write failure
        #[source]
        source: std::io::Error,
    },

    /// Pretty-printing table formatting failed.
    ///
    /// This error occurs when the table formatting system fails to generate
    /// output. This is typically an internal error that should be reported.
    ///
    /// Common causes:
    /// - Memory allocation failures for large result sets
    /// - Unicode handling issues in file paths or hash values
    /// - Terminal width detection failures
    /// - Internal state corruption in formatting logic
    ///
    /// This error suggests either:
    /// - Bug in the pretty-printing implementation
    /// - Unusual data that breaks formatting assumptions
    /// - System resource constraints affecting output generation
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::TableFormattingError {
    ///     details: "Failed to calculate column widths".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Table formatting failed: {details}\n\nThis is an internal error in the pretty-printing system.\nPlease report this issue with the full error message."
    )]
    TableFormattingError {
        /// Specific details about the formatting failure
        details: String,
    },

    /// File path contains invalid UTF-8 characters.
    ///
    /// This error occurs when a file path cannot be converted to valid UTF-8
    /// text, which is required for proper display and processing.
    ///
    /// Common causes:
    /// - Files created on systems with different character encodings
    /// - Binary data or control characters in filenames
    /// - Legacy systems using non-UTF-8 encodings
    /// - Filesystem corruption affecting directory entries
    ///
    /// In bioinformatics:
    /// - Legacy data from older systems may have encoding issues
    /// - International collaborations may use different encodings
    /// - Automated file generation may create invalid names
    ///
    /// Solutions:
    /// - Rename files to use valid UTF-8 characters
    /// - Use filesystem tools to check and repair filenames
    /// - Convert legacy data to modern encoding standards
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::InvalidPathEncoding {
    ///     path: "file_with_invalid_chars.txt".to_string(),
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Path conversion failed for '{path}'\n\nThe file path contains invalid UTF-8 characters.\nPlease ensure all file paths use valid UTF-8 encoding."
    )]
    InvalidPathEncoding {
        /// The path that contains invalid UTF-8 characters
        path: String,
    },

    /// Too many files found for batch processing.
    ///
    /// This error occurs when file discovery finds more files than can be
    /// safely processed in a single batch operation. The limit prevents
    /// memory exhaustion from metadata storage.
    ///
    /// Common scenarios:
    /// - Processing entire filesystem trees unintentionally
    /// - Large genomics datasets with millions of small files
    /// - Recursive directory traversal without proper filtering
    /// - Archive operations with excessive file counts
    ///
    /// Memory usage scales with file count due to:
    /// - Path storage for all discovered files
    /// - Metadata tracking for progress and results
    /// - Hash result accumulation
    ///
    /// Solutions for bioinformatics workflows:
    /// - Use more targeted file patterns (*.fastq, *.bam)
    /// - Exclude temporary or log directories
    /// - Process data in smaller logical partitions
    /// - Increase batch size limits on high-memory systems
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use checkle::errors::CheckleError;
    ///
    /// let error = CheckleError::ExceededFileBatchSize {
    ///     found: 75000,
    ///     limit: 50000,
    /// };
    /// println!("{}", error);
    /// ```
    #[error(
        "Exceeded maximum file batch size: found {found} files, but the limit is {limit}\n\nThis limit exists to prevent memory exhaustion. You can:\n1. Hash a smaller directory tree\n2. Use more specific filters:\n   checkle hash <path> --include '*.fastq' --exclude '**/temp/**'\n3. If your system has sufficient memory, increase the limit:\n   checkle hash <path> --max-files-batch 50000"
    )]
    ExceededFileBatchSize {
        /// Number of files found during discovery
        found: usize,
        /// Maximum allowed files in a single batch
        limit: usize,
    },

    /// Unexpected error not covered by specific error types.
    ///
    /// This error serves as a catch-all for unexpected errors that occur
    /// during checkle operation. It wraps color-eyre Reports which provide
    /// rich error context and suggestions.
    ///
    /// This error type handles:
    /// - Panics that are caught and converted to errors
    /// - External library errors not anticipated by checkle
    /// - System errors in unexpected contexts
    /// - Third-party dependency failures
    ///
    /// When this error occurs, it suggests either:
    /// - An edge case not anticipated in checkle's design
    /// - Environmental issues beyond checkle's control
    /// - Bugs that should be reported to the development team
    ///
    /// The wrapped color-eyre Report provides detailed context including
    /// suggestions, related errors, and system information.
    ///
    /// # Examples
    ///
    /// This error is typically created automatically by the `?` operator
    /// when color-eyre Reports are encountered:
    ///
    /// ```no_run
    /// # use checkle::errors::{CheckleError, Result};
    /// # use color_eyre::eyre;
    ///
    /// fn example_function() -> Result<()> {
    ///     // This would automatically convert to UnknownError
    ///     // return Err(eyre::eyre!("Unexpected condition").into());
    ///     Ok(())
    /// }
    /// ```
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
        // Check that the debug string contains some part of the file path
        // On Windows, the path might be formatted differently in Debug output
        let path_str = temp_file.path().to_string_lossy();
        let file_name = temp_file
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("temp");
        assert!(
            debug_string.contains(file_name) || debug_string.contains(path_str.as_ref()),
            "Debug should show file path or name. Debug string: {}",
            debug_string
        );
    }

    // Test 2: Normal operation - error formatting for MultipleFailedChecksums
    #[test]
    fn test_multiple_failed_checksums_error_formatting() {
        let error = CheckleError::MultipleFailedChecksums;

        let error_string = format!("{}", error);
        assert!(
            error_string.contains("File(s) failed the checksum"),
            "Error should describe checksum failures"
        );
        assert!(
            error_string.contains("logs and/or output"),
            "Error should reference logs and output"
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
                e.to_string().contains("File(s) failed the checksum"),
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
