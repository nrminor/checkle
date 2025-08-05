//! Centralized constants for the checkle library.
//!
//! This module contains all compile-time constants used throughout the library,
//! organized by their purpose and with appropriate visibility for library consumers.

// ============================================================================
// Hashing Constants
// ============================================================================

/// The primary chunk size for file hashing operations (1MB).
/// This is the size used for dividing files into chunks for Merkle tree computation.
pub const CHUNK_SIZE: usize = 1024 * 1024;

/// Default chunk size for parallel I/O operations (256KB).
/// This smaller size provides better granularity for parallel readers.
pub const DEFAULT_CHUNK_SIZE: usize = 256 * 1024;

/// Minimum allowed chunk size (4KB).
/// This ensures chunks are large enough to be efficient.
pub const MIN_CHUNK_SIZE: usize = 4 * 1024;

/// Maximum allowed chunk size (64MB).
/// This prevents excessive memory usage per chunk.
pub const MAX_CHUNK_SIZE: usize = 64 * 1024 * 1024;

/// Maximum number of parallel readers allowed.
/// This prevents resource exhaustion from too many concurrent file handles.
pub const MAX_PARALLEL_READERS: usize = 64;

/// Size of MD5 hash output in bytes.
pub(crate) const MD5_SIZE: usize = 16;

/// Size of SHA-256 hash output in bytes.
pub(crate) const SHA_SIZE: usize = 32;

/// Maximum number of chunks allowed in a single hashing operation.
/// This prevents memory exhaustion from extremely large files.
pub(crate) const MAX_CHUNK_COUNT: usize = 1024 * 1024; // 1M chunks max

/// Maximum number of files in a batch operation.
/// This is used by both hashing and I/O modules.
pub const MAX_FILES_IN_BATCH: usize = 10_000;

/// Minimum allowed value for `max_files_batch` CLI argument.
pub const MIN_FILES_BATCH_LIMIT: usize = 1;

/// Maximum allowed value for `max_files_batch` CLI argument.
/// This prevents users from setting unreasonable values that could exhaust memory.
pub const MAX_FILES_BATCH_LIMIT: usize = 1_000_000;

/// Threshold for enabling parallel I/O (1MB).
/// Files smaller than this use sequential processing.
pub(crate) const PARALLEL_IO_THRESHOLD: u64 = 1024 * 1024;

// ============================================================================
// Buffer Pool Constants
// ============================================================================

/// Maximum number of buffers in the pool.
pub(crate) const MAX_POOL_CAPACITY: usize = 256;

/// Maximum size of a single buffer (64MB).
pub(crate) const MAX_BUFFER_SIZE: usize = 64 * 1024 * 1024;

/// Maximum total memory for all buffers (1GB).
pub(crate) const MAX_TOTAL_MEMORY: usize = 1024 * 1024 * 1024;

/// Memory page size for alignment (4KB).
pub(crate) const PAGE_SIZE: usize = 4096;

// ============================================================================
// Progress Display Constants
// ============================================================================

/// Maximum progress bar update frequency (20Hz).
pub(crate) const MAX_PROGRESS_UPDATE_HZ: u64 = 20;

/// Minimum file size to show individual file progress bars (100MB).
/// Files smaller than this only contribute to the overall progress bar.
pub const MIN_FILE_SIZE_FOR_PROGRESS: u64 = 100 * 1024 * 1024;

/// Progress bars are shown automatically when verbosity level is above Error.
/// Error level maps to filter value 1, Warn level maps to filter value 2.
pub const PROGRESS_VISIBILITY_THRESHOLD: u8 = 1;

/// Progress update interval in milliseconds.
pub(crate) const PROGRESS_UPDATE_INTERVAL_MS: u64 = 1000 / MAX_PROGRESS_UPDATE_HZ;

// ============================================================================
// I/O Operation Constants
// ============================================================================

/// Maximum number of lines in a checksum file.
pub(crate) const MAX_CHECKSUM_FILE_LINES: usize = 100_000;

/// Maximum recursion depth for directory traversal.
pub(crate) const MAX_RECURSION_DEPTH: usize = 100;

// ============================================================================
// Pretty Printing Constants
// ============================================================================

/// Maximum file size for pretty printing (1TB).
/// This prevents memory exhaustion when formatting extremely large files.
pub(crate) const MAX_FILE_SIZE_PRETTY: u64 = 1_099_511_627_776; // 1TB maximum file size (1024^4 bytes)

/// Maximum string length for pretty printing (1MB).
/// This prevents excessive memory usage when formatting large strings.
pub(crate) const MAX_STRING_LENGTH_PRETTY: usize = 1_000_000; // 1MB maximum string length

/// Maximum number of file pairs to process in pretty printing.
/// This ensures reasonable memory usage for large batches.
pub(crate) const MAX_PAIRS_COUNT_PRETTY: usize = 100_000; // Maximum number of file pairs to process

/// Maximum timestamp for pretty printing (2100-01-01 00:00:00 UTC).
/// This provides a reasonable future limit for timestamp validation.
pub(crate) const MAX_TIMESTAMP_PRETTY: u64 = 4_102_444_800; // 2100-01-01 00:00:00 UTC (reasonable future limit)

/// Minimum timestamp for pretty printing (Unix epoch start).
#[allow(dead_code)]
pub(crate) const MIN_TIMESTAMP_PRETTY: u64 = 0; // Unix epoch start

/// Valid Unix permission bits mask.
pub(crate) const VALID_PERMISSION_MASK: u32 = 0o777; // Valid Unix permission bits

/// Expected length of permission string.
pub(crate) const EXPECTED_PERMISSION_STRING_LENGTH: usize = 9; // Expected length of permission string

/// File size units for pretty printing.
pub(crate) const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

/// Threshold for file size unit conversion.
pub(crate) const THRESHOLD: f64 = 1024.0;

// ============================================================================
// Pretty Printing Display Constants
// ============================================================================

/// Maximum length for hash display before truncation (show first 8 and last 8 chars).
pub(crate) const MAX_HASH_DISPLAY_LENGTH: usize = 32;

/// Number of characters to show at start and end of truncated hash.
pub(crate) const HASH_TRUNCATE_CHARS: usize = 8;

/// Maximum path length for display before truncation.
pub(crate) const MAX_PATH_DISPLAY_LENGTH: usize = 50;

/// Number of characters to show at end of truncated path.
pub(crate) const PATH_TRUNCATE_CHARS: usize = 47;

/// Maximum error message length in table display.
pub(crate) const MAX_ERROR_MESSAGE_LENGTH: usize = 48;

/// Minimum length required for string truncation with ellipsis.
pub(crate) const MIN_TRUNCATE_LENGTH: usize = 3;

// ============================================================================
// Archive Operation Constants
// ============================================================================

/// Maximum path length for file paths (4KB).
/// This includes both regular file paths and paths within archives.
pub const MAX_PATH_LENGTH: usize = 4096;

/// Maximum depth of nested directories within archives.
/// This prevents excessive resource usage from deeply nested structures.
pub const MAX_ARCHIVE_DEPTH: usize = 50;

// ============================================================================
// Compile-time Assertions
// ============================================================================

// Chunk size assertions
const _: () = assert!(CHUNK_SIZE > 0, "CHUNK_SIZE must be positive");
const _: () = assert!(
    CHUNK_SIZE >= MIN_CHUNK_SIZE,
    "CHUNK_SIZE must be >= MIN_CHUNK_SIZE"
);
const _: () = assert!(
    CHUNK_SIZE <= MAX_CHUNK_SIZE,
    "CHUNK_SIZE must be <= MAX_CHUNK_SIZE"
);
const _: () = assert!(
    CHUNK_SIZE.is_multiple_of(512),
    "CHUNK_SIZE should be a multiple of 512 for disk alignment"
);

// Default chunk size assertions
const _: () = assert!(
    DEFAULT_CHUNK_SIZE > 0,
    "DEFAULT_CHUNK_SIZE must be positive"
);
const _: () = assert!(
    DEFAULT_CHUNK_SIZE >= MIN_CHUNK_SIZE,
    "DEFAULT_CHUNK_SIZE must be >= MIN_CHUNK_SIZE"
);
const _: () = assert!(
    DEFAULT_CHUNK_SIZE <= MAX_CHUNK_SIZE,
    "DEFAULT_CHUNK_SIZE must be <= MAX_CHUNK_SIZE"
);
const _: () = assert!(
    DEFAULT_CHUNK_SIZE <= CHUNK_SIZE,
    "DEFAULT_CHUNK_SIZE should be <= CHUNK_SIZE"
);

// Min/Max chunk size assertions
const _: () = assert!(MIN_CHUNK_SIZE > 0, "MIN_CHUNK_SIZE must be positive");
const _: () = assert!(
    MIN_CHUNK_SIZE <= DEFAULT_CHUNK_SIZE,
    "MIN_CHUNK_SIZE must be <= DEFAULT_CHUNK_SIZE"
);
const _: () = assert!(
    MIN_CHUNK_SIZE >= 1024,
    "MIN_CHUNK_SIZE should be at least 1KB"
);
const _: () = assert!(
    MIN_CHUNK_SIZE.is_multiple_of(512),
    "MIN_CHUNK_SIZE should be a multiple of 512"
);

const _: () = assert!(MAX_CHUNK_SIZE > 0, "MAX_CHUNK_SIZE must be positive");
const _: () = assert!(
    MAX_CHUNK_SIZE >= DEFAULT_CHUNK_SIZE,
    "MAX_CHUNK_SIZE must be >= DEFAULT_CHUNK_SIZE"
);
const _: () = assert!(
    MAX_CHUNK_SIZE <= 1024 * 1024 * 1024,
    "MAX_CHUNK_SIZE should be <= 1GB"
);
const _: () = assert!(
    MAX_CHUNK_SIZE.is_multiple_of(PAGE_SIZE),
    "MAX_CHUNK_SIZE should be page-aligned"
);

// Parallel readers assertions
const _: () = assert!(
    MAX_PARALLEL_READERS > 0,
    "MAX_PARALLEL_READERS must be positive"
);
const _: () = assert!(
    MAX_PARALLEL_READERS <= 1024,
    "MAX_PARALLEL_READERS should be reasonable (<= 1024)"
);

// Hash size assertions
const _: () = assert!(MD5_SIZE == 16, "MD5 produces 16-byte hashes");
const _: () = assert!(MD5_SIZE > 0, "MD5_SIZE must be positive");

const _: () = assert!(SHA_SIZE == 32, "SHA-256 produces 32-byte hashes");
const _: () = assert!(SHA_SIZE > MD5_SIZE, "SHA-256 hashes are larger than MD5");

// Batch and limit assertions
const _: () = assert!(MAX_CHUNK_COUNT > 0, "MAX_CHUNK_COUNT must be positive");
const _: () = assert!(
    MAX_CHUNK_COUNT >= 1000,
    "MAX_CHUNK_COUNT should handle large files"
);

const _: () = assert!(
    MAX_FILES_IN_BATCH > 0,
    "MAX_FILES_IN_BATCH must be positive"
);
const _: () = assert!(
    MAX_FILES_IN_BATCH >= 100,
    "MAX_FILES_IN_BATCH should handle typical directories"
);

// Max files batch limit assertions
const _: () = assert!(
    MIN_FILES_BATCH_LIMIT > 0,
    "MIN_FILES_BATCH_LIMIT must be positive"
);
const _: () = assert!(
    MAX_FILES_BATCH_LIMIT >= MIN_FILES_BATCH_LIMIT,
    "MAX_FILES_BATCH_LIMIT must be >= MIN_FILES_BATCH_LIMIT"
);
const _: () = assert!(
    MAX_FILES_BATCH_LIMIT >= MAX_FILES_IN_BATCH,
    "MAX_FILES_BATCH_LIMIT must be >= default MAX_FILES_IN_BATCH"
);
const _: () = assert!(
    MAX_FILES_BATCH_LIMIT <= 10_000_000,
    "MAX_FILES_BATCH_LIMIT should be reasonable (<= 10M files)"
);
const _: () = assert!(
    MIN_FILES_BATCH_LIMIT <= MAX_FILES_IN_BATCH,
    "MIN_FILES_BATCH_LIMIT must allow default value"
);

// Buffer pool assertions
const _: () = assert!(MAX_POOL_CAPACITY > 0, "MAX_POOL_CAPACITY must be positive");
const _: () = assert!(
    MAX_POOL_CAPACITY <= 10000,
    "MAX_POOL_CAPACITY should be reasonable"
);

const _: () = assert!(MAX_BUFFER_SIZE > 0, "MAX_BUFFER_SIZE must be positive");
const _: () = assert!(
    MAX_BUFFER_SIZE >= CHUNK_SIZE,
    "MAX_BUFFER_SIZE must be >= CHUNK_SIZE"
);
const _: () = assert!(
    MAX_BUFFER_SIZE.is_multiple_of(PAGE_SIZE),
    "MAX_BUFFER_SIZE should be page-aligned"
);

const _: () = assert!(MAX_TOTAL_MEMORY > 0, "MAX_TOTAL_MEMORY must be positive");
const _: () = assert!(
    MAX_TOTAL_MEMORY >= MAX_BUFFER_SIZE,
    "MAX_TOTAL_MEMORY must fit at least one buffer"
);

const _: () = assert!(PAGE_SIZE > 0, "PAGE_SIZE must be positive");
const _: () = assert!(
    PAGE_SIZE == 4096 || PAGE_SIZE == 8192 || PAGE_SIZE == 16384,
    "PAGE_SIZE should be standard"
);

// Progress display assertions
const _: () = assert!(
    MAX_PROGRESS_UPDATE_HZ > 0,
    "MAX_PROGRESS_UPDATE_HZ must be positive"
);
const _: () = assert!(
    MAX_PROGRESS_UPDATE_HZ <= 60,
    "MAX_PROGRESS_UPDATE_HZ should be <= 60Hz"
);

const _: () = assert!(
    MIN_FILE_SIZE_FOR_PROGRESS > 0,
    "MIN_FILE_SIZE_FOR_PROGRESS must be positive"
);
const _: () = assert!(
    MIN_FILE_SIZE_FOR_PROGRESS >= 1024 * 1024,
    "Progress bars for files >= 1MB"
);

const _: () = assert!(
    PROGRESS_UPDATE_INTERVAL_MS > 0,
    "PROGRESS_UPDATE_INTERVAL_MS must be positive"
);
const _: () = assert!(
    PROGRESS_UPDATE_INTERVAL_MS >= 16,
    "PROGRESS_UPDATE_INTERVAL_MS should be >= 16ms"
);

const _: () = assert!(
    PROGRESS_VISIBILITY_THRESHOLD >= 1,
    "PROGRESS_VISIBILITY_THRESHOLD must be >= Error level (1)"
);
const _: () = assert!(
    PROGRESS_VISIBILITY_THRESHOLD <= 5,
    "PROGRESS_VISIBILITY_THRESHOLD must be <= Trace level (5)"
);

// I/O operation assertions
const _: () = assert!(
    MAX_CHECKSUM_FILE_LINES > 0,
    "MAX_CHECKSUM_FILE_LINES must be positive"
);
const _: () = assert!(
    MAX_CHECKSUM_FILE_LINES >= 1000,
    "MAX_CHECKSUM_FILE_LINES should handle typical files"
);

const _: () = assert!(
    MAX_RECURSION_DEPTH > 0,
    "MAX_RECURSION_DEPTH must be positive"
);
const _: () = assert!(
    MAX_RECURSION_DEPTH <= 1000,
    "MAX_RECURSION_DEPTH should prevent stack overflow"
);

// Parallel I/O threshold assertions
const _: () = assert!(
    PARALLEL_IO_THRESHOLD > 0,
    "PARALLEL_IO_THRESHOLD must be positive"
);
const _: () = assert!(
    PARALLEL_IO_THRESHOLD >= MIN_CHUNK_SIZE as u64,
    "PARALLEL_IO_THRESHOLD >= MIN_CHUNK_SIZE"
);

// Pretty printing assertions
const _: () = assert!(UNITS.len() == 5, "UNITS array must have exactly 5 elements");
const _: () = assert!(
    MAX_FILE_SIZE_PRETTY > 0,
    "MAX_FILE_SIZE_PRETTY must be positive"
);
const _: () = assert!(
    MAX_STRING_LENGTH_PRETTY > 0,
    "MAX_STRING_LENGTH_PRETTY must be positive"
);
const _: () = assert!(
    MAX_PAIRS_COUNT_PRETTY > 0,
    "MAX_PAIRS_COUNT_PRETTY must be positive"
);
const _: () = assert!(
    MAX_TIMESTAMP_PRETTY > MIN_TIMESTAMP_PRETTY,
    "MAX_TIMESTAMP_PRETTY must be greater than MIN_TIMESTAMP_PRETTY"
);
const _: () = assert!(THRESHOLD > 1.0, "THRESHOLD must be greater than 1.0");
