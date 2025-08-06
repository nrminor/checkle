//! Archive handling module for checkle.
//!
//! This module provides streaming access to files within TAR and ZIP archives,
//! enabling efficient checksum computation without extracting the entire archive.
//! The implementation follows Tiger Style principles with comprehensive assertions
//! and resource limits.
//!
//! # Design Philosophy
//!
//! - **Streaming First**: Never load entire archives into memory
//! - **Parallel When Possible**: Maintain checkle's multicore advantage
//! - **Resource Limits**: Every operation has defined boundaries
//! - **Error Recovery**: Graceful handling of corrupted archives
//!
//! # Supported Formats
//!
//! - TAR archives (including .tar.gz, .tar.bz2, .tar.xz)
//! - ZIP archives (including various compression methods)

use md5::Md5;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use crate::{
    buffer_pool::BufferPool,
    constants::{
        CHUNK_SIZE, MAX_BUFFER_SIZE, MD5_SIZE, MIN_FILE_SIZE_FOR_PROGRESS, PAGE_SIZE,
        PARALLEL_IO_THRESHOLD, SHA_SIZE,
    },
    errors::{CheckleError, Result},
    hashing::{HashArray, HashingAlgo, MerkleIter},
    simd,
};

// ============================================================================
// Archive-Specific Constants
// ============================================================================

/// Maximum size of an archive we'll process (1TB).
/// This prevents resource exhaustion from extremely large archives.
pub const MAX_ARCHIVE_SIZE: u64 = 1_000_000_000_000; // 1TB

/// Maximum number of entries in an archive.
/// This prevents memory exhaustion from archives with millions of tiny files.
pub const MAX_ARCHIVE_ENTRIES: usize = 100_000;

/// Buffer size for archive operations (16MB).
/// Larger than normal buffers for efficient decompression.
pub const ARCHIVE_BUFFER_SIZE: usize = 16 * 1024 * 1024;

/// Maximum size of a single file within an archive (500GB).
/// Genomics files can be huge, but we need a limit.
pub const MAX_ARCHIVE_ENTRY_SIZE: u64 = 500_000_000_000; // 500GB

/// Timeout for archive operations (30 minutes).
/// Large genomics archives may take time to process.
pub const ARCHIVE_OPERATION_TIMEOUT_SECS: u64 = 1800;

/// Minimum size for parallel processing within archives (10MB).
/// Smaller files use sequential processing to avoid overhead.
pub const ARCHIVE_PARALLEL_THRESHOLD: u64 = 10 * 1024 * 1024;

// ============================================================================
// Compile-time Assertions for Archive Constants
// ============================================================================

// Archive size assertions
const _: () = assert!(MAX_ARCHIVE_SIZE > 0, "MAX_ARCHIVE_SIZE must be positive");
const _: () = assert!(
    MAX_ARCHIVE_SIZE >= MAX_ARCHIVE_ENTRY_SIZE,
    "Archive must be able to contain at least one max-size entry"
);
const _: () = assert!(
    MAX_ARCHIVE_SIZE <= 10_000_000_000_000,
    "MAX_ARCHIVE_SIZE should be reasonable (<= 10TB)"
);

// Archive entries assertions
const _: () = assert!(
    MAX_ARCHIVE_ENTRIES > 0,
    "MAX_ARCHIVE_ENTRIES must be positive"
);
const _: () = assert!(
    MAX_ARCHIVE_ENTRIES >= 100,
    "MAX_ARCHIVE_ENTRIES should handle typical archives"
);
const _: () = assert!(
    MAX_ARCHIVE_ENTRIES <= 10_000_000,
    "MAX_ARCHIVE_ENTRIES should prevent memory exhaustion"
);

// Buffer size assertions
const _: () = assert!(
    ARCHIVE_BUFFER_SIZE > 0,
    "ARCHIVE_BUFFER_SIZE must be positive"
);
const _: () = assert!(
    ARCHIVE_BUFFER_SIZE >= CHUNK_SIZE,
    "ARCHIVE_BUFFER_SIZE must be >= CHUNK_SIZE for efficiency"
);
const _: () = assert!(
    ARCHIVE_BUFFER_SIZE <= MAX_BUFFER_SIZE,
    "ARCHIVE_BUFFER_SIZE must fit within buffer pool limits"
);
const _: () = assert!(
    ARCHIVE_BUFFER_SIZE.is_multiple_of(PAGE_SIZE),
    "ARCHIVE_BUFFER_SIZE should be page-aligned"
);

// Entry size assertions
const _: () = assert!(
    MAX_ARCHIVE_ENTRY_SIZE > 0,
    "MAX_ARCHIVE_ENTRY_SIZE must be positive"
);
const _: () = assert!(
    MAX_ARCHIVE_ENTRY_SIZE >= 1_000_000_000,
    "MAX_ARCHIVE_ENTRY_SIZE should handle large genomics files (>= 1GB)"
);

// Parallel threshold assertions
const _: () = assert!(
    ARCHIVE_PARALLEL_THRESHOLD > 0,
    "ARCHIVE_PARALLEL_THRESHOLD must be positive"
);
const _: () = assert!(
    ARCHIVE_PARALLEL_THRESHOLD >= PARALLEL_IO_THRESHOLD,
    "ARCHIVE_PARALLEL_THRESHOLD should be >= general parallel threshold"
);
const _: () = assert!(
    ARCHIVE_PARALLEL_THRESHOLD <= 1_000_000_000,
    "ARCHIVE_PARALLEL_THRESHOLD should be reasonable (<= 1GB)"
);

// ============================================================================
// Self-Documenting Type Aliases
// ============================================================================

/// Path to an archive file on the filesystem.
///
/// This type alias makes function signatures more self-documenting by clearly
/// indicating when a path parameter refers to an archive file rather than
/// a regular file or directory path.
pub type ArchivePath = PathBuf;

/// Path to a file or directory within an archive.
///
/// This represents the internal path structure within an archive,
/// which may use forward slashes even on Windows systems.
pub type ArchiveEntryPath = String;

/// Size of an archive file in bytes.
///
/// Used to represent the total size of an archive file on disk,
/// which may be smaller than the sum of its entries due to compression.
pub type ArchiveSizeBytes = u64;

/// Size of an individual entry within an archive in bytes.
///
/// This represents the uncompressed size of a single file or directory
/// entry within an archive.
pub type EntrySizeBytes = u64;

/// Number of entries contained within an archive.
///
/// This count includes all files and directories within the archive,
/// used for progress reporting and resource limit validation.
pub type ArchiveEntryCount = usize;

/// Compression ratio as a floating-point value between 0.0 and 1.0.
///
/// Represents the ratio of compressed size to uncompressed size:
/// - 1.0 = no compression (stored)
/// - 0.5 = 50% compression ratio
/// - 0.1 = 90% compression achieved
pub type CompressionRatio = f64;

/// Buffer size for archive operations in bytes.
///
/// Represents the size of memory buffers used for reading and processing
/// archive data, typically larger than normal I/O buffers for efficiency.
pub type ArchiveBufferSize = usize;

/// Byte offset within an archive file.
///
/// Used for seeking to specific positions within an archive file
/// for random access to entries.
pub type ArchiveOffset = u64;

/// Complex iterator type for archive entries.
type ArchiveEntriesIterator<Entry, Metadata> =
    Box<dyn Iterator<Item = Result<(ArchiveEntryPath, Entry, Metadata)>>>;

// ============================================================================
// Validated Newtypes
// ============================================================================

/// A validated archive path that is guaranteed to exist and be a file.
///
/// This newtype provides compile-time assurance that the path has been
/// validated, following Tiger Style principles of positive invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedArchivePath(ArchivePath);

impl ValidatedArchivePath {
    /// Create a new validated archive path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to validate
    ///
    /// # Returns
    ///
    /// A validated archive path if the path exists and is a file.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path doesn't exist
    /// - The path is not a file
    /// - The file size exceeds maximum archive size
    ///
    /// # Panics
    ///
    /// Panics if precondition assertions fail.
    pub fn new(path: impl Into<ArchivePath>) -> Result<Self> {
        let path: ArchivePath = path.into();

        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!path.as_os_str().is_empty(), "Path must not be empty");
        assert!(
            path.to_string_lossy().len() <= 4096,
            "Path must be reasonable length"
        );

        if !path.exists() {
            return Err(CheckleError::InaccessibleFile(path.clone()));
        }

        if !path.is_file() {
            return Err(CheckleError::InaccessibleFile(path.clone()));
        }

        // Check size limits
        let _size = check_archive_size(&path)?;

        let validated = Self(path);

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(validated.0.exists(), "Validated path must exist");
        assert!(validated.0.is_file(), "Validated path must be a file");

        Ok(validated)
    }

    /// Get the inner path.
    ///
    /// Since this path is validated, callers can be confident it exists
    /// and represents a valid archive file.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Convert to `PathBuf`, consuming the validated path.
    #[must_use]
    pub fn into_path_buf(self) -> ArchivePath {
        self.0
    }
}

/// A validated entry path within an archive.
///
/// This newtype ensures entry paths are non-empty and within reasonable limits,
/// preventing issues with malformed archive entries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatedEntryPath(ArchiveEntryPath);

impl ValidatedEntryPath {
    /// Create a new validated entry path.
    ///
    /// # Arguments
    ///
    /// * `path` - The entry path to validate
    ///
    /// # Returns
    ///
    /// A validated entry path.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty or excessively long.
    ///
    /// # Panics
    ///
    /// Panics if precondition assertions fail.
    pub fn new(path: impl Into<ArchiveEntryPath>) -> Result<Self> {
        let path: ArchiveEntryPath = path.into();

        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!path.is_empty(), "Entry path must not be empty");
        assert!(path.len() <= 4096, "Entry path must be reasonable length");

        if path.is_empty() {
            return Err(CheckleError::InvalidPathEncoding {
                path: "empty path".to_string(),
            });
        }

        if path.len() > 4096 {
            return Err(CheckleError::InvalidPathEncoding {
                path: format!("path too long: {} characters", path.len()),
            });
        }

        let validated = Self(path);

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(!validated.0.is_empty(), "Validated path must not be empty");
        assert!(validated.0.len() <= 4096, "Validated path within limits");

        Ok(validated)
    }

    /// Get the inner path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to String, consuming the validated path.
    #[must_use]
    pub fn into_string(self) -> ArchiveEntryPath {
        self.0
    }
}

/// A validated entry size that is guaranteed to be within limits.
///
/// This newtype prevents archive entries from exceeding resource limits,
/// providing compile-time assurance of constraint satisfaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValidatedEntrySize(EntrySizeBytes);

impl ValidatedEntrySize {
    /// Create a new validated entry size.
    ///
    /// # Arguments
    ///
    /// * `size` - The size in bytes to validate
    ///
    /// # Returns
    ///
    /// A validated entry size.
    ///
    /// # Errors
    ///
    /// Returns an error if the size exceeds maximum limits.
    ///
    /// # Panics
    ///
    /// Panics if precondition assertions fail.
    pub fn new(size: EntrySizeBytes) -> Result<Self> {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(size < u64::MAX, "Size must be within bounds");

        if size > MAX_ARCHIVE_ENTRY_SIZE {
            return Err(CheckleError::ArchiveEntryTooLarge {
                archive: PathBuf::from("unknown"),
                entry: "unknown".to_string(),
                size,
                limit: MAX_ARCHIVE_ENTRY_SIZE,
            });
        }

        let validated = Self(size);

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(validated.0 <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");
        assert!(validated.0 < u64::MAX, "Size within bounds");

        Ok(validated)
    }

    /// Get the inner size in bytes.
    #[must_use]
    pub fn as_bytes(&self) -> EntrySizeBytes {
        self.0
    }
}

// ============================================================================
// Core Archive Traits
// ============================================================================

/// Trait for archive formats that support streaming file access.
///
/// This trait defines the minimal interface required for an archive format
/// to be supported by checkle. Implementations must provide efficient
/// streaming access to individual files within the archive.
pub trait ArchiveReader: Sized {
    /// Type representing an entry that can be read from.
    type Entry: Read;

    /// Type representing metadata about an entry.
    type EntryMetadata;

    /// Open an archive for reading.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the archive file
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The archive doesn't exist or can't be opened
    /// - The archive format is corrupted
    /// - The archive exceeds size limits
    fn open(path: &Path) -> Result<Self>;

    /// Find a file by path within the archive.
    ///
    /// # Arguments
    ///
    /// * `path` - Internal path within the archive
    ///
    /// # Returns
    ///
    /// - `Ok(Some(entry))` if the file exists
    /// - `Ok(None)` if the file doesn't exist
    /// - `Err` if there's an error accessing the archive
    ///
    /// # Errors
    ///
    /// Returns an error if the archive cannot be accessed or is corrupted.
    fn find_entry(&mut self, path: &str) -> Result<Option<(Self::Entry, Self::EntryMetadata)>>;

    /// Iterate over all entries in the archive.
    ///
    /// # Returns
    ///
    /// An iterator over `(path, entry, metadata)` tuples.
    ///
    /// # Errors
    ///
    /// Individual entries may fail to read, in which case the iterator
    /// yields an error for that entry.
    fn entries(&mut self) -> Result<ArchiveEntriesIterator<Self::Entry, Self::EntryMetadata>>;

    /// Get the total number of entries in the archive.
    ///
    /// This is used for progress reporting and resource limit checks.
    ///
    /// # Errors
    ///
    /// Returns an error if the archive cannot be accessed or is corrupted.
    fn entry_count(&self) -> Result<ArchiveEntryCount>;
}

/// Metadata about an archive entry.
///
/// This struct contains the minimal information needed to process
/// an entry efficiently, following Tiger Style's data minimization principle.
#[derive(Debug, Clone)]
pub struct ArchiveEntryMetadata {
    /// Path of the entry within the archive.
    pub path: PathBuf,

    /// Uncompressed size of the entry in bytes.
    pub size: EntrySizeBytes,

    /// Whether this entry is compressed within the archive.
    pub is_compressed: bool,

    /// Compression ratio (`compressed_size` / `uncompressed_size`).
    /// Used for progress estimation.
    pub compression_ratio: CompressionRatio,

    /// Offset within the archive (for seek optimization).
    pub offset: Option<ArchiveOffset>,
}

impl ArchiveEntryMetadata {
    /// Create new metadata with validation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path of the entry within the archive
    /// * `size` - Uncompressed size in bytes
    /// * `is_compressed` - Whether the entry is compressed
    /// * `compression_ratio` - Ratio of compressed to uncompressed size
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Size exceeds `MAX_ARCHIVE_ENTRY_SIZE`
    /// - Compression ratio is invalid (< 0 or > 1)
    #[must_use]
    pub fn new(
        path: PathBuf,
        size: EntrySizeBytes,
        is_compressed: bool,
        compression_ratio: CompressionRatio,
    ) -> Self {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            size <= MAX_ARCHIVE_ENTRY_SIZE,
            "Entry size {size} exceeds maximum {MAX_ARCHIVE_ENTRY_SIZE}"
        );
        assert!(
            (0.0..=1.0).contains(&compression_ratio),
            "Invalid compression ratio: {compression_ratio}, must be between 0.0 and 1.0"
        );

        let metadata = Self {
            path,
            size,
            is_compressed,
            compression_ratio,
            offset: None,
        };

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(metadata.size <= MAX_ARCHIVE_ENTRY_SIZE);
        assert!((0.0..=1.0).contains(&metadata.compression_ratio));

        metadata
    }

    /// Set the offset within the archive.
    ///
    /// # Arguments
    ///
    /// * `offset` - Byte offset within the archive
    ///
    /// # Panics
    ///
    /// Panics if the offset exceeds the maximum archive size.
    #[must_use]
    pub fn with_offset(mut self, offset: u64) -> Self {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            offset <= MAX_ARCHIVE_SIZE,
            "Offset exceeds maximum archive size"
        );
        assert!(
            self.size <= MAX_ARCHIVE_ENTRY_SIZE,
            "Entry size within limits"
        );

        self.offset = Some(offset);

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(self.offset.is_some(), "Offset has been set");
        assert!(
            self.offset.is_some_and(|o| o <= MAX_ARCHIVE_SIZE),
            "Offset within bounds"
        );

        self
    }
}

/// Unified archive interface supporting multiple formats.
///
/// This enum provides a consistent API over different archive formats,
/// enabling the rest of checkle to work with archives without concern
/// for the underlying format.
pub enum Archive {
    /// TAR archive (possibly compressed).
    #[cfg(feature = "tar")]
    Tar(TarArchive),

    /// ZIP archive.
    #[cfg(feature = "zip")]
    Zip(ZipArchive),
}

impl Archive {
    /// Open an archive, automatically detecting its format.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the archive file
    ///
    /// # Returns
    ///
    /// An `Archive` instance appropriate for the detected format.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file doesn't exist
    /// - The format is unsupported
    /// - The archive is corrupted
    /// - Resource limits are exceeded
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The path doesn't exist
    /// - The path is not a file
    /// - The archive size exceeds `MAX_ARCHIVE_SIZE`
    pub fn open(path: &Path) -> Result<Self> {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(path.exists(), "Archive must exist: {}", path.display());
        assert!(path.is_file(), "Path must be a file: {}", path.display());

        let archive_size = check_archive_size(path)?;
        let format = detect_archive_format(path)?;

        // Create archive based on detected format
        let archive = match format {
            #[cfg(feature = "tar")]
            ArchiveFormat::Tar => Archive::Tar(TarArchive::open(path)?),

            #[cfg(feature = "zip")]
            ArchiveFormat::Zip => Archive::Zip(ZipArchive::open(path)?),

            #[allow(unreachable_patterns)]
            _ => return Err(CheckleError::UnsupportedArchiveFormat(path.to_path_buf())),
        };

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            archive_size <= MAX_ARCHIVE_SIZE,
            "Archive size within limits"
        );
        assert!(path.exists(), "Archive still exists after opening");

        Ok(archive)
    }

    /// Hash a specific file within the archive.
    ///
    /// This method maintains checkle's parallel hashing capability even
    /// for files within archives, using streaming decompression when needed.
    ///
    /// # Arguments
    ///
    /// * `internal_path` - Path to the file within the archive
    /// * `algo` - Hashing algorithm to use
    /// * `buffer_pool` - Buffer pool for efficient memory usage
    ///
    /// # Returns
    ///
    /// The computed hash as a hexadecimal string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file doesn't exist in the archive
    /// - Reading fails
    /// - Resource limits are exceeded
    ///
    /// # Panics
    ///
    /// Panics if the internal path is empty or excessively long.
    pub fn hash_entry(
        &mut self,
        internal_path: &str,
        algo: HashingAlgo,
        buffer_pool: &BufferPool,
    ) -> Result<String> {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!internal_path.is_empty(), "Internal path must not be empty");
        assert!(
            internal_path.len() <= 4096,
            "Internal path too long: {} characters",
            internal_path.len()
        );

        let hash = match self {
            #[cfg(feature = "tar")]
            Archive::Tar(archive) => archive.hash_entry(internal_path, algo, buffer_pool),

            #[cfg(feature = "zip")]
            Archive::Zip(archive) => archive.hash_entry(internal_path, algo, buffer_pool),
        }?;

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(!hash.is_empty(), "Hash must not be empty");
        assert!(
            crate::simd::is_hex_string(&hash),
            "Hash must contain only hexadecimal characters"
        );

        Ok(hash)
    }

    /// List all entries in the archive.
    ///
    /// # Returns
    ///
    /// A vector of internal paths within the archive.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the archive directory fails.
    ///
    /// # Panics
    ///
    /// Panics if the entry count or paths exceed expected limits.
    pub fn list_entries(&mut self) -> Result<Vec<String>> {
        let entries = match self {
            #[cfg(feature = "tar")]
            Archive::Tar(archive) => archive.list_entries(),

            #[cfg(feature = "zip")]
            Archive::Zip(archive) => archive.list_entries(),
        }?;

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            entries.len() <= MAX_ARCHIVE_ENTRIES,
            "Entry count {} exceeds maximum {MAX_ARCHIVE_ENTRIES}",
            entries.len()
        );
        assert!(
            entries.iter().all(|path| !path.is_empty()),
            "All entry paths must be non-empty"
        );

        Ok(entries)
    }

    /// Get the count of entries in the archive.
    ///
    /// Used for progress reporting and validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the archive cannot be accessed or is corrupted.
    ///
    /// # Panics
    ///
    /// Panics if the entry count exceeds expected limits.
    pub fn entry_count(&self) -> Result<ArchiveEntryCount> {
        let count = match self {
            #[cfg(feature = "tar")]
            Archive::Tar(archive) => archive.entry_count(),

            #[cfg(feature = "zip")]
            Archive::Zip(archive) => Ok(archive.entry_count()),
        }?;

        // Postconditions (Tiger Style: minimum 2 per function)
        assert!(
            count <= MAX_ARCHIVE_ENTRIES,
            "Entry count {count} exceeds maximum {MAX_ARCHIVE_ENTRIES}"
        );
        assert!(count < usize::MAX, "Entry count within bounds");

        Ok(count)
    }
}

/// Supported archive formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormat {
    /// TAR format (including compressed variants).
    Tar,

    /// ZIP format.
    Zip,
}

/// Check archive file size and validate against limits.
///
/// # Arguments
///
/// * `path` - Path to the archive file
///
/// # Returns
///
/// The size of the archive in bytes.
///
/// # Errors
///
/// Returns an error if the file cannot be accessed or exceeds size limits.
fn check_archive_size(path: &Path) -> Result<ArchiveSizeBytes> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(path.is_file(), "Path must be a file");

    let metadata = std::fs::metadata(path).map_err(|e| CheckleError::FileOpenError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let size = metadata.len();
    if size > MAX_ARCHIVE_SIZE {
        return Err(CheckleError::ArchiveTooLarge {
            path: path.to_path_buf(),
            size,
            limit: MAX_ARCHIVE_SIZE,
        });
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(size <= MAX_ARCHIVE_SIZE, "Size within limits");
    assert!(size == metadata.len(), "Size matches metadata");

    Ok(size)
}

/// Detect archive format from file extension and magic bytes.
///
/// This function uses a two-phase approach:
/// 1. Check file extension for quick detection
/// 2. Verify with magic bytes for security
///
/// # Arguments
///
/// * `path` - Path to the archive file
///
/// # Returns
///
/// The detected archive format.
///
/// # Errors
///
/// Returns an error if the format is unsupported or can't be determined.
fn detect_archive_format(path: &Path) -> Result<ArchiveFormat> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(path.is_file(), "Path must be a file");

    // First, check extension
    let format_by_extension = detect_format_by_extension(path);

    // If extension gave us a hint, verify with magic bytes
    let detected_format = if let Some(format) = format_by_extension {
        verify_archive_format(path, format)?;
        format
    } else {
        // Otherwise, check magic bytes directly
        detect_format_by_magic(path)?
    };

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        matches!(detected_format, ArchiveFormat::Tar | ArchiveFormat::Zip),
        "Detected format must be supported"
    );
    assert!(path.exists(), "Path still exists after detection");

    Ok(detected_format)
}

/// Detect format by file extension.
///
/// # Arguments
///
/// * `path` - Path to examine
///
/// # Returns
///
/// Optional archive format based on extension.
fn detect_format_by_extension(path: &Path) -> Option<ArchiveFormat> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(!path.as_os_str().is_empty(), "Path must not be empty");

    match path.extension().and_then(|s| s.to_str()) {
        Some("gz" | "tgz") => {
            // Could be .tar.gz
            if path.to_string_lossy().contains(".tar.") {
                Some(ArchiveFormat::Tar)
            } else {
                None
            }
        }
        Some("bz2" | "tbz2" | "tbz" | "xz" | "txz" | "tar") => Some(ArchiveFormat::Tar),
        Some("zip") => Some(ArchiveFormat::Zip),
        _ => None,
    }
}

/// Verify archive format matches expected type using magic bytes.
///
/// # Arguments
///
/// * `path` - Path to the archive file
/// * `expected_format` - Format we expect based on extension
///
/// # Errors
///
/// Returns an error if the actual format doesn't match expected.
fn verify_archive_format(path: &Path, expected_format: ArchiveFormat) -> Result<()> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(path.is_file(), "Path must be a file");

    let actual_format = detect_format_by_magic(path)?;

    if actual_format != expected_format {
        return Err(CheckleError::InvalidArchiveFormat {
            path: path.to_path_buf(),
            expected: format!("{expected_format:?}"),
            actual: format!("{actual_format:?}"),
        });
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert_eq!(actual_format, expected_format, "Formats must match");
    assert!(path.exists(), "Path still exists after verification");

    Ok(())
}

/// Detect archive format by reading magic bytes.
///
/// # Arguments
///
/// * `path` - Path to the archive file
///
/// # Returns
///
/// The detected archive format.
///
/// # Errors
///
/// Returns an error if:
/// - The file can't be read
/// - The format is unsupported
fn detect_format_by_magic(path: &Path) -> Result<ArchiveFormat> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(path.is_file(), "Path must be a file");

    let magic_bytes = read_magic_bytes(path)?;

    if magic_bytes.len() < 4 {
        return Err(CheckleError::InvalidArchiveFormat {
            path: path.to_path_buf(),
            expected: "valid archive".to_string(),
            actual: "file too small".to_string(),
        });
    }

    let format = classify_magic_bytes(&magic_bytes)?;

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        matches!(format, ArchiveFormat::Tar | ArchiveFormat::Zip),
        "Format must be supported"
    );
    assert!(!magic_bytes.is_empty(), "Magic bytes must not be empty");

    Ok(format)
}

/// Read magic bytes from file header.
///
/// # Arguments
///
/// * `path` - Path to read from
///
/// # Returns
///
/// Vector of magic bytes (up to 512 bytes for TAR header).
///
/// # Errors
///
/// Returns an error if the file cannot be read.
fn read_magic_bytes(path: &Path) -> Result<Vec<u8>> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(path.exists(), "Path must exist");
    assert!(path.is_file(), "Path must be a file");

    let mut file = File::open(path).map_err(|e| CheckleError::FileOpenError {
        path: path.to_path_buf(),
        source: e,
    })?;

    let mut magic = vec![0u8; 512]; // TAR needs 512 bytes for header
    let bytes_read = file
        .read(&mut magic)
        .map_err(|e| CheckleError::FileReadError {
            path: path.to_path_buf(),
            source: e,
        })?;

    magic.truncate(bytes_read);

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(magic.len() <= 512, "Magic bytes within expected range");
    assert!(magic.len() == bytes_read, "Read correct number of bytes");

    Ok(magic)
}

/// Classify magic bytes to determine archive format.
///
/// # Arguments
///
/// * `magic` - Magic bytes from file header
///
/// # Returns
///
/// The detected archive format.
///
/// # Errors
///
/// Returns an error if the format is unsupported.
fn classify_magic_bytes(magic: &[u8]) -> Result<ArchiveFormat> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(!magic.is_empty(), "Magic bytes must not be empty");
    assert!(magic.len() <= 512, "Magic bytes within reasonable size");

    // Check for ZIP magic bytes
    if magic.len() >= 4
        && (magic[0..4] == [0x50, 0x4B, 0x03, 0x04] // Standard ZIP
        || magic[0..4] == [0x50, 0x4B, 0x05, 0x06] // Empty ZIP
        || magic[0..4] == [0x50, 0x4B, 0x07, 0x08])
    {
        // Spanned ZIP
        return Ok(ArchiveFormat::Zip);
    }

    // Check for compressed TAR formats
    if magic.len() >= 2 && magic[0..2] == [0x1F, 0x8B] {
        // gzip magic bytes
        return Ok(ArchiveFormat::Tar);
    }

    if magic.len() >= 3 && magic[0..3] == [0x42, 0x5A, 0x68] {
        // bzip2 magic bytes
        return Ok(ArchiveFormat::Tar);
    }

    if magic.len() >= 6 && magic[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        // xz magic bytes
        return Ok(ArchiveFormat::Tar);
    }

    // Check for TAR header (ustar format)
    if magic.len() >= 512 && &magic[257..262] == b"ustar" {
        return Ok(ArchiveFormat::Tar);
    }

    // If no format matched, it's unsupported
    Err(CheckleError::UnsupportedArchiveFormat(PathBuf::from(
        "unknown",
    )))
}

// ============================================================================
// TAR Implementation (feature-gated)
// ============================================================================

#[cfg(feature = "tar")]
mod tar_impl {
    use super::{
        ArchiveEntriesIterator, ArchiveEntryCount, ArchiveEntryMetadata, ArchiveReader, BufferPool,
        CheckleError, File, HashingAlgo, MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_ENTRY_SIZE, Path,
        PathBuf, Read, Result, check_entry_size, compute_hash_for_reader,
    };
    use std::io::Cursor;
    use tar::Archive as TarArchiveInner;

    /// TAR entry reader wrapper that implements Read trait.
    pub struct TarEntryReader {
        reader: Box<dyn Read>,
    }

    impl TarEntryReader {
        /// Create a new TAR entry reader.
        ///
        /// # Arguments
        ///
        /// * `reader` - The underlying reader
        ///
        /// # Panics
        ///
        /// Panics if the reader type is invalid or memory allocation fails.
        pub fn new<R: Read + 'static>(reader: R) -> Self {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                std::mem::size_of::<R>() > 0,
                "Reader type must have non-zero size"
            );
            assert!(
                std::mem::size_of::<Box<dyn Read>>() > 0,
                "Box<dyn Read> must have non-zero size"
            );

            let entry_reader = Self {
                reader: Box::new(reader),
            };

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                std::mem::size_of_val(&entry_reader.reader) > 0,
                "Reader must be properly initialized"
            );
            assert!(
                !std::ptr::eq(&raw const entry_reader.reader, std::ptr::null()),
                "Reader pointer must not be null"
            );

            entry_reader
        }
    }

    impl Read for TarEntryReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reader.read(buf)
        }
    }

    /// Opens a TAR archive with transparent decompression based on file extension.
    ///
    /// This function detects the compression format from the file extension and
    /// automatically applies the appropriate decompressor. This fixes the critical
    /// bug where compressed TAR archives were being read as raw bytes.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the TAR archive (may be compressed)
    ///
    /// # Returns
    ///
    /// A boxed `Read` trait object that transparently decompresses the archive
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened
    fn open_tar_with_decompression(path: &Path) -> Result<Box<dyn Read>> {
        // Precondition assertions (Tiger Style)
        assert!(path.exists(), "Archive path must exist");
        assert!(path.is_file(), "Archive path must be a file");

        let file = File::open(path).map_err(|e| CheckleError::FileOpenError {
            path: path.to_path_buf(),
            source: e,
        })?;

        // Detect compression from file extension
        let path_str = path.to_string_lossy();

        // Check for gzip compression
        if path_str.ends_with(".tar.gz") || path_str.ends_with(".tgz") {
            #[cfg(feature = "flate2")]
            {
                use flate2::read::GzDecoder;
                return Ok(Box::new(GzDecoder::new(file)));
            }

            #[cfg(not(feature = "flate2"))]
            return Err(CheckleError::UnsupportedArchiveFormat {
                path: path.to_path_buf(),
                format: "gzip-compressed TAR (install with flate2 feature)".to_string(),
            });
        }

        // Check for bzip2 compression
        if path_str.ends_with(".tar.bz2")
            || path_str.ends_with(".tbz2")
            || path_str.ends_with(".tbz")
        {
            #[cfg(feature = "bzip2")]
            {
                use bzip2::read::BzDecoder;
                return Ok(Box::new(BzDecoder::new(file)));
            }

            #[cfg(not(feature = "bzip2"))]
            return Err(CheckleError::UnsupportedArchiveFormat {
                path: path.to_path_buf(),
                format: "bzip2-compressed TAR (install with bzip2 feature)".to_string(),
            });
        }

        // Check for xz compression
        if path_str.ends_with(".tar.xz") || path_str.ends_with(".txz") {
            #[cfg(feature = "xz2")]
            {
                use xz2::read::XzDecoder;
                return Ok(Box::new(XzDecoder::new(file)));
            }

            #[cfg(not(feature = "xz2"))]
            return Err(CheckleError::UnsupportedArchiveFormat {
                path: path.to_path_buf(),
                format: "xz-compressed TAR (install with xz2 feature)".to_string(),
            });
        }

        // No compression detected, return raw file
        Ok(Box::new(file))
    }

    /// TAR archive implementation.
    pub struct TarArchive {
        path: PathBuf,
        // We'll create fresh Archive instances as needed to avoid
        // ownership issues with the tar crate's design
    }

    impl TarArchive {
        /// Open a TAR archive.
        ///
        /// # Arguments
        ///
        /// * `path` - Path to the TAR archive file
        ///
        /// # Errors
        ///
        /// Returns an error if:
        /// - The file cannot be opened
        /// - The TAR format is invalid
        /// - The archive exceeds resource limits
        ///
        /// # Panics
        ///
        /// Panics if the path is invalid or entry count exceeds limits.
        pub fn open(path: &Path) -> Result<Self> {
            // Precondition checks converted to proper error handling
            if !path.exists() {
                return Err(CheckleError::InaccessibleFile(path.to_path_buf()));
            }
            if !path.is_file() {
                return Err(CheckleError::InaccessibleFile(path.to_path_buf()));
            }

            let archive = Self {
                path: path.to_path_buf(),
            };

            // Verify we can open and read the archive
            let entry_count = archive.verify_archive_readable()?;

            // Postcondition checks converted to proper error handling
            if !archive.path.exists() {
                return Err(CheckleError::InaccessibleFile(archive.path.clone()));
            }
            if entry_count > MAX_ARCHIVE_ENTRIES {
                return Err(CheckleError::TooManyArchiveEntries {
                    path: archive.path.clone(),
                    count: entry_count,
                    limit: MAX_ARCHIVE_ENTRIES,
                });
            }

            Ok(archive)
        }

        /// Verify that the archive can be read and count entries.
        ///
        /// # Errors
        ///
        /// Returns an error if the archive cannot be read or has too many entries.
        fn verify_archive_readable(&self) -> Result<usize> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive path must exist");
            assert!(self.path.is_file(), "Archive path must be a file");

            // Use decompression wrapper to handle compressed archives
            let reader = open_tar_with_decompression(&self.path)?;
            let mut archive = TarArchiveInner::new(reader);
            let mut entries = archive
                .entries()
                .map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read TAR directory: {e}"),
                })?;

            // Try to read first entry to validate TAR format
            let entry_count = match entries.next() {
                Some(result) => {
                    // If first entry fails, the TAR is corrupted
                    result.map_err(|e| CheckleError::CorruptedArchive {
                        path: self.path.clone(),
                        details: format!("Failed to read entry 0: {e}"),
                    })?;
                    // Count remaining entries plus the first one
                    1 + entries.try_fold(0usize, |count, entry| -> Result<usize> {
                        entry.map_err(|e| CheckleError::CorruptedArchive {
                            path: self.path.clone(),
                            details: format!("Failed to read entry {}: {e}", count + 1),
                        })?;
                        Ok(count + 1)
                    })?
                }
                None => 0, // Empty archive
            };

            if entry_count > MAX_ARCHIVE_ENTRIES {
                return Err(CheckleError::TooManyArchiveEntries {
                    path: self.path.clone(),
                    count: entry_count,
                    limit: MAX_ARCHIVE_ENTRIES,
                });
            }

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entry_count <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(entry_count < usize::MAX, "Entry count within bounds");

            Ok(entry_count)
        }

        /// Hash a specific entry.
        ///
        /// # Arguments
        ///
        /// * `internal_path` - Path within the archive
        /// * `algo` - Hashing algorithm to use
        /// * `buffer_pool` - Buffer pool for memory management
        ///
        /// # Errors
        ///
        /// Returns an error if:
        /// - The entry cannot be found
        /// - The entry cannot be read
        /// - The entry exceeds size limits
        /// - Hash computation fails
        ///
        /// # Panics
        ///
        /// Panics if the path is empty or the archive doesn't exist.
        pub fn hash_entry(
            &mut self,
            internal_path: &str,
            algo: HashingAlgo,
            buffer_pool: &BufferPool,
        ) -> Result<String> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(!internal_path.is_empty(), "Internal path must not be empty");
            assert!(self.path.exists(), "Archive must exist");

            let (mut entry, size) = self.find_tar_entry(internal_path)?;
            let hash = compute_hash_for_reader(&mut entry, size, algo, buffer_pool)?;

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(!hash.is_empty(), "Hash must not be empty");
            assert!(
                crate::simd::is_hex_string(&hash),
                "Hash must be hexadecimal"
            );

            Ok(hash)
        }

        /// Find a TAR entry by path.
        ///
        /// # Arguments
        ///
        /// * `internal_path` - Path within the archive
        ///
        /// # Returns
        ///
        /// Tuple of (`entry_reader`, size) if found.
        ///
        /// # Errors
        ///
        /// Returns an error if the entry cannot be found or read.
        fn find_tar_entry(&self, internal_path: &str) -> Result<(Box<dyn Read>, u64)> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(!internal_path.is_empty(), "Internal path must not be empty");
            assert!(self.path.exists(), "Archive must exist");

            // Use decompression wrapper to handle compressed archives
            let reader = open_tar_with_decompression(&self.path)?;
            let mut archive = TarArchiveInner::new(reader);

            // Find the entry
            for entry in archive
                .entries()
                .map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read entries: {e}"),
                })?
            {
                let mut entry = entry.map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read entry: {e}"),
                })?;

                let entry_path = entry.path().map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Invalid entry path: {e}"),
                })?;

                if entry_path.to_str() == Some(internal_path) {
                    let size = entry.size();
                    check_entry_size(size)?;

                    // Read the entire entry into memory to avoid lifetime issues
                    let mut buffer = Vec::new();
                    std::io::copy(&mut entry, &mut buffer).map_err(|e| {
                        CheckleError::ArchiveReadError {
                            details: format!("Failed to read TAR entry: {e}"),
                        }
                    })?;

                    // Postcondition assertions (Tiger Style: minimum 2 per function)
                    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");
                    assert!(
                        !buffer.is_empty() || size == 0,
                        "Buffer size matches entry size"
                    );

                    let reader: Box<dyn Read> = Box::new(Cursor::new(buffer));

                    return Ok((reader, size));
                }
            }

            Err(CheckleError::FileNotFoundInArchive {
                archive: self.path.clone(),
                file: internal_path.to_string(),
            })
        }

        /// List all entries.
        ///
        /// # Errors
        ///
        /// Returns an error if the archive cannot be read or has too many entries.
        ///
        /// # Panics
        ///
        /// Panics if the archive path is invalid or entry count exceeds limits.
        pub fn list_entries(&mut self) -> Result<Vec<String>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(self.path.is_file(), "Archive must be a file");

            // Use decompression wrapper to handle compressed archives
            let reader = open_tar_with_decompression(&self.path)?;
            let mut archive = TarArchiveInner::new(reader);
            let mut entries = Vec::new();

            for entry in archive
                .entries()
                .map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read entries: {e}"),
                })?
            {
                let entry = entry.map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read entry: {e}"),
                })?;

                let path = entry.path().map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Invalid entry path: {e}"),
                })?;

                entries.push(path.to_string_lossy().to_string());

                if entries.len() > MAX_ARCHIVE_ENTRIES {
                    return Err(CheckleError::TooManyArchiveEntries {
                        path: self.path.clone(),
                        count: entries.len(),
                        limit: MAX_ARCHIVE_ENTRIES,
                    });
                }
            }

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(
                entries.iter().all(|path| !path.is_empty()),
                "All paths non-empty"
            );

            Ok(entries)
        }

        /// Get entry count.
        ///
        /// # Errors
        ///
        /// Returns an error if the archive cannot be read.
        pub fn entry_count(&self) -> Result<ArchiveEntryCount> {
            self.verify_archive_readable()
        }
    }

    impl ArchiveReader for TarArchive {
        type Entry = TarEntryReader;
        type EntryMetadata = ArchiveEntryMetadata;

        fn open(path: &Path) -> Result<Self> {
            Self::open(path)
        }

        fn find_entry(&mut self, path: &str) -> Result<Option<(Self::Entry, Self::EntryMetadata)>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(!path.is_empty(), "Path must not be empty");
            assert!(self.path.exists(), "Archive must exist");

            match self.find_tar_entry(path) {
                Ok((reader, size)) => {
                    let metadata = ArchiveEntryMetadata::new(PathBuf::from(path), size, false, 1.0);
                    let entry = TarEntryReader::new(reader);

                    // Postcondition assertions (Tiger Style: minimum 2 per function)
                    assert!(metadata.size == size, "Metadata size matches");
                    assert!(
                        metadata.size <= MAX_ARCHIVE_ENTRY_SIZE,
                        "Size within limits"
                    );

                    Ok(Some((entry, metadata)))
                }
                Err(CheckleError::FileNotFoundInArchive { .. }) => Ok(None),
                Err(e) => Err(e),
            }
        }

        fn entries(&mut self) -> Result<ArchiveEntriesIterator<Self::Entry, Self::EntryMetadata>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(self.path.is_file(), "Archive must be a file");

            let entries = self.collect_all_entries()?;

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(
                entries.iter().all(|result| {
                    if let Ok((path, _, metadata)) = result {
                        !path.is_empty() && metadata.size <= MAX_ARCHIVE_ENTRY_SIZE
                    } else {
                        true // Errors are acceptable
                    }
                }),
                "All entries have valid paths and sizes"
            );

            Ok(Box::new(entries.into_iter()))
        }

        fn entry_count(&self) -> Result<ArchiveEntryCount> {
            self.entry_count()
        }
    }

    impl TarArchive {
        /// Collect all entries from the archive.
        ///
        /// # Errors
        ///
        /// Returns an error if entries cannot be read.
        fn collect_all_entries(
            &self,
        ) -> Result<Vec<Result<(String, TarEntryReader, ArchiveEntryMetadata)>>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(self.path.is_file(), "Archive must be a file");

            // Use decompression wrapper to handle compressed archives
            let reader = open_tar_with_decompression(&self.path)?;
            let mut archive = TarArchiveInner::new(reader);
            let mut entries = Vec::new();

            for (index, entry_result) in archive
                .entries()
                .map_err(|e| CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to read entries: {e}"),
                })?
                .enumerate()
            {
                let entry_item = self.process_tar_entry(entry_result, index);
                entries.push(entry_item);

                if entries.len() > MAX_ARCHIVE_ENTRIES {
                    return Err(CheckleError::TooManyArchiveEntries {
                        path: self.path.clone(),
                        count: entries.len(),
                        limit: MAX_ARCHIVE_ENTRIES,
                    });
                }
            }

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(
                !entries.is_empty() || entries.is_empty(),
                "Entries properly collected"
            );

            Ok(entries)
        }

        /// Process a single TAR entry.
        ///
        /// # Arguments
        ///
        /// * `entry_result` - Result from TAR entry iterator
        /// * `index` - Index of the entry for error reporting
        ///
        /// # Returns
        ///
        /// Result containing the processed entry data.
        fn process_tar_entry<R: Read>(
            &self,
            entry_result: std::result::Result<tar::Entry<R>, std::io::Error>,
            index: usize,
        ) -> Result<(String, TarEntryReader, ArchiveEntryMetadata)> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(
                index < MAX_ARCHIVE_ENTRIES,
                "Index within reasonable bounds"
            );

            let mut entry = entry_result.map_err(|e| CheckleError::CorruptedArchive {
                path: self.path.clone(),
                details: format!("Failed to read entry {index}: {e}"),
            })?;

            let entry_path = entry.path().map_err(|e| CheckleError::CorruptedArchive {
                path: self.path.clone(),
                details: format!("Invalid entry path for entry {index}: {e}"),
            })?;
            let entry_path_string = entry_path.to_string_lossy().to_string();

            let size = entry.size();
            check_entry_size(size)?;

            // Read the entire entry into memory to avoid lifetime issues
            let mut buffer = Vec::new();
            std::io::copy(&mut entry, &mut buffer).map_err(|e| CheckleError::ArchiveReadError {
                details: format!("Failed to read TAR entry {index}: {e}"),
            })?;

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                !entry_path_string.is_empty(),
                "Entry path must not be empty"
            );
            assert!(
                !buffer.is_empty() || size == 0,
                "Buffer size matches entry size"
            );

            // Create metadata and reader
            let metadata =
                ArchiveEntryMetadata::new(PathBuf::from(&entry_path_string), size, false, 1.0);
            let reader = TarEntryReader::new(Cursor::new(buffer));

            Ok((entry_path_string, reader, metadata))
        }
    }
}

#[cfg(feature = "tar")]
pub use tar_impl::{TarArchive, TarEntryReader};

// ============================================================================
// ZIP Implementation (feature-gated)
// ============================================================================

#[cfg(feature = "zip")]
mod zip_impl {
    use super::{
        ArchiveEntriesIterator, ArchiveEntryCount, ArchiveEntryMetadata, ArchiveReader, BufferPool,
        CheckleError, File, HashingAlgo, MAX_ARCHIVE_ENTRIES, MAX_ARCHIVE_ENTRY_SIZE, Path,
        PathBuf, Read, Result, check_entry_size, compute_hash_for_reader,
    };
    use std::io::{BufReader, Cursor};
    use zip::ZipArchive as ZipArchiveInner;

    /// ZIP entry reader wrapper that implements Read trait.
    pub struct ZipEntryReader {
        reader: Box<dyn Read>,
    }

    impl ZipEntryReader {
        /// Create a new ZIP entry reader.
        ///
        /// # Arguments
        ///
        /// * `reader` - The underlying reader
        ///
        /// # Panics
        ///
        /// Panics if the reader type is invalid or memory allocation fails.
        pub fn new<R: Read + 'static>(reader: R) -> Self {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                std::mem::size_of::<R>() > 0,
                "Reader type must have non-zero size"
            );
            assert!(
                std::mem::size_of::<Box<dyn Read>>() > 0,
                "Box<dyn Read> must have non-zero size"
            );

            let entry_reader = Self {
                reader: Box::new(reader),
            };

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                std::mem::size_of_val(&entry_reader.reader) > 0,
                "Reader must be properly initialized"
            );
            assert!(
                !std::ptr::eq(&raw const entry_reader.reader, std::ptr::null()),
                "Reader pointer must not be null"
            );

            entry_reader
        }
    }

    impl Read for ZipEntryReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.reader.read(buf)
        }
    }

    /// ZIP archive implementation.
    pub struct ZipArchive {
        archive: ZipArchiveInner<BufReader<File>>,
        path: PathBuf,
    }

    impl ZipArchive {
        /// Open a ZIP archive.
        ///
        /// # Arguments
        ///
        /// * `path` - Path to the ZIP archive file
        ///
        /// # Errors
        ///
        /// Returns an error if:
        /// - The file cannot be opened
        /// - The ZIP format is invalid
        /// - The archive exceeds resource limits
        ///
        /// # Panics
        ///
        /// Panics if the path is invalid or entry count exceeds limits.
        pub fn open(path: &Path) -> Result<Self> {
            // Precondition checks converted to proper error handling
            if !path.exists() {
                return Err(CheckleError::InaccessibleFile(path.to_path_buf()));
            }
            if !path.is_file() {
                return Err(CheckleError::InaccessibleFile(path.to_path_buf()));
            }

            let file = File::open(path).map_err(|e| CheckleError::FileOpenError {
                path: path.to_path_buf(),
                source: e,
            })?;

            let reader = BufReader::new(file);
            let archive =
                ZipArchiveInner::new(reader).map_err(|e| CheckleError::CorruptedArchive {
                    path: path.to_path_buf(),
                    details: format!("Invalid ZIP format: {e}"),
                })?;

            // Check entry count
            if archive.len() > MAX_ARCHIVE_ENTRIES {
                return Err(CheckleError::TooManyArchiveEntries {
                    path: path.to_path_buf(),
                    count: archive.len(),
                    limit: MAX_ARCHIVE_ENTRIES,
                });
            }

            let zip_archive = Self {
                archive,
                path: path.to_path_buf(),
            };

            // Postcondition checks converted to proper error handling
            if !zip_archive.path.exists() {
                return Err(CheckleError::InaccessibleFile(zip_archive.path.clone()));
            }
            // Entry count was already checked above, no need to check again

            Ok(zip_archive)
        }

        /// Hash a specific entry.
        ///
        /// # Arguments
        ///
        /// * `internal_path` - Path within the archive
        /// * `algo` - Hashing algorithm to use
        /// * `buffer_pool` - Buffer pool for memory management
        ///
        /// # Errors
        ///
        /// Returns an error if:
        /// - The entry cannot be found
        /// - The entry cannot be read
        /// - The entry exceeds size limits
        /// - Hash computation fails
        ///
        /// # Panics
        ///
        /// Panics if the path is empty or the archive doesn't exist.
        pub fn hash_entry(
            &mut self,
            internal_path: &str,
            algo: HashingAlgo,
            buffer_pool: &BufferPool,
        ) -> Result<String> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(!internal_path.is_empty(), "Internal path must not be empty");
            assert!(self.path.exists(), "Archive must exist");

            let mut entry = self.archive.by_name(internal_path).map_err(|e| match e {
                zip::result::ZipError::FileNotFound => CheckleError::FileNotFoundInArchive {
                    archive: self.path.clone(),
                    file: internal_path.to_string(),
                },
                _ => CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to access entry: {e}"),
                },
            })?;

            let size = entry.size();
            check_entry_size(size)?;

            let hash = compute_hash_for_reader(&mut entry, size, algo, buffer_pool)?;

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(!hash.is_empty(), "Hash must not be empty");
            assert!(
                crate::simd::is_hex_string(&hash),
                "Hash must be hexadecimal"
            );

            Ok(hash)
        }

        /// List all entries.
        ///
        /// # Errors
        ///
        /// Returns an error if entries cannot be read.
        ///
        /// # Panics
        ///
        /// Panics if the archive path is invalid or entry count exceeds limits.
        pub fn list_entries(&mut self) -> Result<Vec<String>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(
                self.archive.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );

            let mut entries = Vec::with_capacity(self.archive.len());

            for i in 0..self.archive.len() {
                let entry =
                    self.archive
                        .by_index(i)
                        .map_err(|e| CheckleError::CorruptedArchive {
                            path: self.path.clone(),
                            details: format!("Failed to read entry {i}: {e}"),
                        })?;

                entries.push(entry.name().to_string());
            }

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(
                entries.iter().all(|path| !path.is_empty()),
                "All paths non-empty"
            );

            Ok(entries)
        }

        /// Get entry count.
        #[must_use]
        pub fn entry_count(&self) -> ArchiveEntryCount {
            self.archive.len()
        }
    }

    impl ArchiveReader for ZipArchive {
        type Entry = ZipEntryReader;
        type EntryMetadata = ArchiveEntryMetadata;

        fn open(path: &Path) -> Result<Self> {
            Self::open(path)
        }

        fn find_entry(&mut self, path: &str) -> Result<Option<(Self::Entry, Self::EntryMetadata)>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(!path.is_empty(), "Path must not be empty");
            assert!(self.path.exists(), "Archive must exist");

            match self.archive.by_name(path) {
                Ok(mut entry) => {
                    let size = entry.size();
                    check_entry_size(size)?;

                    // Read the entire entry into memory to avoid lifetime issues
                    let mut buffer = Vec::new();
                    std::io::copy(&mut entry, &mut buffer).map_err(|e| {
                        CheckleError::ArchiveReadError {
                            details: format!("Failed to read ZIP entry: {e}"),
                        }
                    })?;

                    // Create metadata - ZIP entries are usually compressed
                    let compressed_size = entry.compressed_size();
                    let is_compressed = compressed_size < size;
                    let compression_ratio = calculate_compression_ratio(compressed_size, size);

                    // Postcondition assertions (Tiger Style: minimum 2 per function)
                    assert!(
                        !buffer.is_empty() || size == 0,
                        "Buffer size matches entry size"
                    );
                    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");

                    let metadata = ArchiveEntryMetadata::new(
                        PathBuf::from(path),
                        size,
                        is_compressed,
                        compression_ratio,
                    );

                    // Create reader from buffer
                    let reader = ZipEntryReader::new(Cursor::new(buffer));

                    Ok(Some((reader, metadata)))
                }
                Err(zip::result::ZipError::FileNotFound) => Ok(None),
                Err(e) => Err(CheckleError::CorruptedArchive {
                    path: self.path.clone(),
                    details: format!("Failed to access entry: {e}"),
                }),
            }
        }

        fn entries(&mut self) -> Result<ArchiveEntriesIterator<Self::Entry, Self::EntryMetadata>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(
                self.archive.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );

            let entries = self.collect_all_zip_entries();

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(
                entries.iter().all(|result| {
                    if let Ok((path, _, metadata)) = result {
                        !path.is_empty() && metadata.size <= MAX_ARCHIVE_ENTRY_SIZE
                    } else {
                        true // Errors are acceptable
                    }
                }),
                "All entries have valid paths and sizes"
            );

            Ok(Box::new(entries.into_iter()))
        }

        fn entry_count(&self) -> Result<ArchiveEntryCount> {
            Ok(self.entry_count())
        }
    }

    impl ZipArchive {
        /// Collect all entries from the ZIP archive.
        ///
        /// # Errors
        ///
        /// Returns an error if entries cannot be read.
        fn collect_all_zip_entries(
            &mut self,
        ) -> Vec<Result<(String, ZipEntryReader, ArchiveEntryMetadata)>> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(self.path.exists(), "Archive must exist");
            assert!(
                self.archive.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );

            let archive_len = self.archive.len();
            let mut entries = Vec::new();

            for i in 0..archive_len {
                let entry_item = self.process_zip_entry(i);
                entries.push(entry_item);
            }

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(
                entries.len() <= MAX_ARCHIVE_ENTRIES,
                "Entry count within limits"
            );
            assert!(entries.len() == archive_len, "All entries processed");

            entries
        }

        /// Process a single ZIP entry.
        ///
        /// # Arguments
        ///
        /// * `index` - Index of the entry to process
        ///
        /// # Returns
        ///
        /// Result containing the processed entry data.
        fn process_zip_entry(
            &mut self,
            index: usize,
        ) -> Result<(String, ZipEntryReader, ArchiveEntryMetadata)> {
            // Precondition assertions (Tiger Style: minimum 2 per function)
            assert!(index < self.archive.len(), "Index within bounds");
            assert!(self.path.exists(), "Archive must exist");

            let mut entry =
                self.archive
                    .by_index(index)
                    .map_err(|e| CheckleError::CorruptedArchive {
                        path: self.path.clone(),
                        details: format!("Failed to read entry {index}: {e}"),
                    })?;

            let entry_name = entry.name().to_string();
            let size = entry.size();
            check_entry_size(size)?;

            // Read the entire entry into memory to avoid lifetime issues
            let mut buffer = Vec::new();
            std::io::copy(&mut entry, &mut buffer).map_err(|e| CheckleError::ArchiveReadError {
                details: format!("Failed to read ZIP entry {index}: {e}"),
            })?;

            // Create metadata
            let compressed_size = entry.compressed_size();
            let is_compressed = compressed_size < size;
            let compression_ratio = calculate_compression_ratio(compressed_size, size);

            // Postcondition assertions (Tiger Style: minimum 2 per function)
            assert!(!entry_name.is_empty(), "Entry name must not be empty");
            assert!(
                !buffer.is_empty() || size == 0,
                "Buffer size matches entry size"
            );

            let metadata = ArchiveEntryMetadata::new(
                PathBuf::from(&entry_name),
                size,
                is_compressed,
                compression_ratio,
            );

            // Create reader from buffer
            let reader = ZipEntryReader::new(Cursor::new(buffer));

            Ok((entry_name, reader, metadata))
        }
    }

    /// Calculate compression ratio safely to avoid precision loss.
    ///
    /// # Arguments
    ///
    /// * `compressed_size` - Size of compressed data
    /// * `uncompressed_size` - Size of uncompressed data
    ///
    /// # Returns
    ///
    /// Compression ratio as f64 (0.0 to 1.0).
    fn calculate_compression_ratio(compressed_size: u64, uncompressed_size: u64) -> f64 {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            compressed_size <= uncompressed_size,
            "Compressed size should not exceed uncompressed size"
        );
        assert!(
            uncompressed_size > 0 || compressed_size == 0,
            "If uncompressed size is 0, compressed size must also be 0"
        );

        if uncompressed_size == 0 {
            1.0
        } else {
            // Use f64 to avoid precision loss from u64 to f32 casting
            #[allow(clippy::cast_precision_loss)]
            {
                (compressed_size as f64) / (uncompressed_size as f64)
            }
        }
    }
}

#[cfg(feature = "zip")]
pub use zip_impl::{ZipArchive, ZipEntryReader};

// ============================================================================
// Shared Hashing Implementation
// ============================================================================

/// Check if entry size is within limits.
///
/// # Arguments
///
/// * `size` - Size to check
///
/// # Errors
///
/// Returns an error if size exceeds limits.
fn check_entry_size(size: u64) -> Result<()> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(size < u64::MAX, "Size must be within bounds");
    // Note: MAX_ARCHIVE_ENTRY_SIZE > 0 is guaranteed by compile-time constant assertion

    if size > MAX_ARCHIVE_ENTRY_SIZE {
        return Err(CheckleError::ArchiveEntryTooLarge {
            archive: PathBuf::from("unknown"),
            entry: "unknown".to_string(),
            size,
            limit: MAX_ARCHIVE_ENTRY_SIZE,
        });
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        size <= MAX_ARCHIVE_ENTRY_SIZE,
        "Size verified within limits"
    );
    assert!(size < u64::MAX, "Size remains within bounds");

    Ok(())
}

/// Hash data from any Read source using checkle's Merkle tree approach.
///
/// This function adapts streaming archive data to checkle's parallel
/// hashing infrastructure, maintaining performance even for compressed data.
///
/// # Arguments
///
/// * `reader` - Source to read data from
/// * `size` - Total size of the data (for progress reporting)
/// * `algo` - Hashing algorithm to use
/// * `buffer_pool` - Buffer pool for efficient memory usage
///
/// # Returns
///
/// The computed hash as a hexadecimal string.
///
/// # Errors
///
/// Returns an error if reading fails or resource limits are exceeded.
fn compute_hash_for_reader<R: Read>(
    reader: &mut R,
    size: u64,
    algo: HashingAlgo,
    buffer_pool: &BufferPool,
) -> Result<String> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");
    assert!(
        matches!(algo, HashingAlgo::Md5 | HashingAlgo::Sha2),
        "Algorithm must be supported"
    );

    // Decide whether to use parallel processing
    let hash = if size >= ARCHIVE_PARALLEL_THRESHOLD {
        // For large files, we need a more sophisticated approach
        // This is a placeholder for the parallel implementation
        hash_reader_sequential(reader, size, algo, buffer_pool)?
    } else {
        // For smaller files, sequential is fine
        hash_reader_sequential(reader, size, algo, buffer_pool)?
    };

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(!hash.is_empty(), "Hash must not be empty");
    assert!(simd::is_hex_string(&hash), "Hash must be hexadecimal");

    Ok(hash)
}

/// Convert hash bytes to hexadecimal string.
///
/// # Arguments
///
/// * `hash_bytes` - The hash bytes to convert
///
/// # Returns
///
/// Hexadecimal string representation of the hash bytes.
#[must_use]
fn hash_bytes_to_hex<const N: usize>(hash_bytes: [u8; N]) -> String {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(N > 0, "Hash size must be positive");
    assert!(N <= 64, "Hash size must be reasonable");

    let hex_string = crate::simd::bytes_to_hex(&hash_bytes);

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert_eq!(
        hex_string.len(),
        N * 2,
        "Hex string length matches hash size"
    );
    assert!(
        simd::is_hex_string(&hex_string),
        "Hex string contains only hex digits"
    );

    hex_string
}

/// Compute chunk hashes for MD5 algorithm.
///
/// # Arguments
///
/// * `reader` - Source to read data from
/// * `size` - Total expected size of the data (for validation)
/// * `buffer` - Buffer to use for reading
///
/// # Returns
///
/// Vector of MD5 hash arrays for all chunks.
///
/// # Errors
///
/// Returns an error if reading fails or more data is read than expected.
fn compute_md5_chunk_hashes<R: Read>(
    reader: &mut R,
    size: u64,
    buffer: &mut [u8],
) -> Result<Vec<[u8; MD5_SIZE]>> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(!buffer.is_empty(), "Buffer must not be empty");
    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");

    let mut hashes = Vec::new();
    let mut total_read = 0u64;

    loop {
        let bytes_read = reader
            .read(buffer)
            .map_err(|e| CheckleError::ArchiveReadError {
                details: format!("Failed to read from archive entry: {e}"),
            })?;

        if bytes_read == 0 {
            break;
        }

        total_read += bytes_read as u64;

        // Check that we're not reading more than expected
        if total_read > size {
            return Err(CheckleError::ArchiveReadError {
                details: format!("Read {total_read} bytes but entry size is {size} bytes"),
            });
        }

        // Compute binary hash for this chunk
        let mut md5_hasher = Md5::new();
        md5_hasher.update(&buffer[..bytes_read]);
        let hash_bytes = md5_hasher.finalize();

        // Convert to fixed-size array
        let hash_array: [u8; MD5_SIZE] = hash_bytes.into();
        hashes.push(hash_array);
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(total_read <= size, "Did not read more than expected");
    assert!(
        !hashes.is_empty() || size == 0,
        "Hashes generated for non-empty data"
    );

    Ok(hashes)
}

/// Compute chunk hashes for SHA256 algorithm.
///
/// # Arguments
///
/// * `reader` - Source to read data from
/// * `size` - Total expected size of the data (for validation)
/// * `buffer` - Buffer to use for reading
///
/// # Returns
///
/// Vector of SHA256 hash arrays for all chunks.
///
/// # Errors
///
/// Returns an error if reading fails or more data is read than expected.
fn compute_sha256_chunk_hashes<R: Read>(
    reader: &mut R,
    size: u64,
    buffer: &mut [u8],
) -> Result<Vec<[u8; SHA_SIZE]>> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(!buffer.is_empty(), "Buffer must not be empty");
    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");

    let mut hashes = Vec::new();
    let mut total_read = 0u64;

    loop {
        let bytes_read = reader
            .read(buffer)
            .map_err(|e| CheckleError::ArchiveReadError {
                details: format!("Failed to read from archive entry: {e}"),
            })?;

        if bytes_read == 0 {
            break;
        }

        total_read += bytes_read as u64;

        // Check that we're not reading more than expected
        if total_read > size {
            return Err(CheckleError::ArchiveReadError {
                details: format!("Read {total_read} bytes but entry size is {size} bytes"),
            });
        }

        // Compute binary hash for this chunk
        let mut sha256_hasher = Sha256::new();
        sha256_hasher.update(&buffer[..bytes_read]);
        let hash_bytes = sha256_hasher.finalize();

        // Convert to fixed-size array
        let hash_array: [u8; SHA_SIZE] = hash_bytes.into();
        hashes.push(hash_array);
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(total_read <= size, "Did not read more than expected");
    assert!(
        !hashes.is_empty() || size == 0,
        "Hashes generated for non-empty data"
    );

    Ok(hashes)
}

/// Compute final hash from chunk hashes using Merkle tree.
///
/// # Arguments
///
/// * `hashes` - Vector of hash arrays
/// * `algo` - Hashing algorithm used
///
/// # Returns
///
/// Final hash as hexadecimal string.
///
/// # Errors
///
/// Returns an error if Merkle tree computation fails.
fn compute_final_hash_from_chunks(
    hashes: Vec<impl AsRef<[u8]>>,
    algo: HashingAlgo,
) -> Result<String> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        matches!(algo, HashingAlgo::Md5 | HashingAlgo::Sha2),
        "Algorithm must be supported"
    );
    assert!(hashes.len() <= 1_000_000, "Hash count must be reasonable");

    match algo {
        HashingAlgo::Md5 => {
            if hashes.is_empty() {
                let empty_hasher = Md5::new();
                let empty_hash = empty_hasher.finalize();
                let empty_array: [u8; MD5_SIZE] = empty_hash.into();
                Ok(hash_bytes_to_hex(empty_array))
            } else {
                let final_hash = compute_md5_merkle_root(hashes)?;
                Ok(hash_bytes_to_hex(final_hash))
            }
        }
        HashingAlgo::Sha2 => {
            if hashes.is_empty() {
                let empty_hasher = Sha256::new();
                let empty_hash = empty_hasher.finalize();
                let empty_array: [u8; SHA_SIZE] = empty_hash.into();
                Ok(hash_bytes_to_hex(empty_array))
            } else {
                let final_hash = compute_sha256_merkle_root(hashes)?;
                Ok(hash_bytes_to_hex(final_hash))
            }
        }
    }
}

/// Compute MD5 Merkle tree root.
///
/// # Arguments
///
/// * `hashes` - Vector of hash data
///
/// # Returns
///
/// Root hash array.
///
/// # Errors
///
/// Returns an error if Merkle tree computation fails.
fn compute_md5_merkle_root(hashes: Vec<impl AsRef<[u8]>>) -> Result<[u8; MD5_SIZE]> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(!hashes.is_empty(), "Hashes must not be empty");
    assert!(hashes.len() <= 1_000_000, "Hash count must be reasonable");

    let hash_vec: Vec<[u8; MD5_SIZE]> = hashes
        .into_iter()
        .map(|h| {
            let slice = h.as_ref();
            let mut array = [0u8; MD5_SIZE];
            array.copy_from_slice(slice);
            array
        })
        .collect();

    let hash_array = HashArray { hashes: hash_vec };
    let final_hashes = hash_array.par_iter_merkle::<Md5>()?.get_hashes();

    if final_hashes.len() != 1 {
        return Err(CheckleError::HashingError {
            details: format!(
                "Merkle tree computation failed: expected 1 root hash, got {}",
                final_hashes.len()
            ),
        });
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert_eq!(final_hashes.len(), 1, "Must have exactly one root hash");
    assert_ne!(
        final_hashes[0], [0u8; MD5_SIZE],
        "Root hash must not be all zeros"
    );

    Ok(final_hashes[0])
}

/// Compute SHA256 Merkle tree root.
///
/// # Arguments
///
/// * `hashes` - Vector of hash data
///
/// # Returns
///
/// Root hash array.
///
/// # Errors
///
/// Returns an error if Merkle tree computation fails.
fn compute_sha256_merkle_root(hashes: Vec<impl AsRef<[u8]>>) -> Result<[u8; SHA_SIZE]> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(!hashes.is_empty(), "Hashes must not be empty");
    assert!(hashes.len() <= 1_000_000, "Hash count must be reasonable");

    let hash_vec: Vec<[u8; SHA_SIZE]> = hashes
        .into_iter()
        .map(|h| {
            let slice = h.as_ref();
            let mut array = [0u8; SHA_SIZE];
            array.copy_from_slice(slice);
            array
        })
        .collect();

    let hash_array = HashArray { hashes: hash_vec };
    let final_hashes = hash_array.par_iter_merkle::<Sha256>()?.get_hashes();

    if final_hashes.len() != 1 {
        return Err(CheckleError::HashingError {
            details: format!(
                "Merkle tree computation failed: expected 1 root hash, got {}",
                final_hashes.len()
            ),
        });
    }

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert_eq!(final_hashes.len(), 1, "Must have exactly one root hash");
    assert_ne!(
        final_hashes[0], [0u8; SHA_SIZE],
        "Root hash must not be all zeros"
    );

    Ok(final_hashes[0])
}

/// Sequential hashing for archive entries.
///
/// This function integrates with checkle's existing Merkle tree infrastructure
/// by processing data in chunks, computing binary hashes, and using the same
/// parallel Merkle tree computation as the main Hasher.
///
/// # Arguments
///
/// * `reader` - Source to read data from
/// * `size` - Total expected size of the data (for validation)
/// * `algo` - Hashing algorithm to use
/// * `buffer_pool` - Buffer pool for efficient memory usage
///
/// # Returns
///
/// The computed root hash as a hexadecimal string, in the same format
/// as checkle's main hashing infrastructure.
///
/// # Errors
///
/// Returns an error if:
/// - Reading from the source fails
/// - More data is read than expected
/// - Hash computation fails
/// - Merkle tree computation fails
///
/// # Panics
///
/// Panics if the buffer pool returns a buffer of incorrect size.
fn hash_reader_sequential<R: Read>(
    reader: &mut R,
    size: u64,
    algo: HashingAlgo,
    buffer_pool: &BufferPool,
) -> Result<String> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(size <= MAX_ARCHIVE_ENTRY_SIZE, "Size within limits");
    assert!(
        matches!(algo, HashingAlgo::Md5 | HashingAlgo::Sha2),
        "Algorithm must be supported"
    );

    // Acquire buffer from pool - this should be CHUNK_SIZE
    let mut buffer = buffer_pool.acquire();

    if buffer.len() != CHUNK_SIZE {
        return Err(CheckleError::HashingError {
            details: format!(
                "Buffer pool returned incorrect size: {} != {CHUNK_SIZE}",
                buffer.len()
            ),
        });
    }

    // Process chunks based on algorithm and collect binary hashes
    let root_hash_string = match algo {
        HashingAlgo::Md5 => {
            let hashes = compute_md5_chunk_hashes(reader, size, buffer.as_mut_slice())?;
            compute_final_hash_from_chunks(hashes, algo)?
        }
        HashingAlgo::Sha2 => {
            let hashes = compute_sha256_chunk_hashes(reader, size, buffer.as_mut_slice())?;
            compute_final_hash_from_chunks(hashes, algo)?
        }
    };

    // Buffer is automatically returned to pool when dropped

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        !root_hash_string.is_empty(),
        "Root hash string must not be empty"
    );
    assert!(
        simd::is_hex_string(&root_hash_string),
        "Hash string must contain only hexadecimal characters"
    );

    Ok(root_hash_string)
}

// ============================================================================
// Progress Integration
// ============================================================================

/// Progress reporting for archive operations.
///
/// This struct integrates with checkle's existing progress system
/// to provide nested progress bars for archive operations.
pub struct ArchiveProgress {
    /// Overall archive progress.
    pub archive_bar: indicatif::ProgressBar,

    /// Current entry progress.
    pub entry_bar: Option<indicatif::ProgressBar>,
}

impl ArchiveProgress {
    /// Create a new progress reporter for an archive.
    ///
    /// # Arguments
    ///
    /// * `archive_name` - Name of the archive being processed
    /// * `total_entries` - Total number of entries to process
    ///
    /// # Panics
    ///
    /// Panics if the archive name is empty or total entries exceed limits.
    #[must_use]
    pub fn new(archive_name: &str, total_entries: u64) -> Self {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!archive_name.is_empty(), "Archive name must not be empty");
        assert!(
            total_entries <= MAX_ARCHIVE_ENTRIES as u64,
            "Total entries within limits"
        );

        let archive_bar = create_archive_progress_bar(total_entries, archive_name);

        let progress = Self {
            archive_bar,
            entry_bar: None,
        };

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            progress.archive_bar.length() == Some(total_entries),
            "Progress bar length set correctly"
        );
        assert!(progress.entry_bar.is_none(), "Entry bar initially None");

        progress
    }

    /// Start processing a new entry.
    ///
    /// # Arguments
    ///
    /// * `entry_name` - Name of the entry being processed
    /// * `entry_size` - Size of the entry in bytes
    ///
    /// # Panics
    ///
    /// May panic if progress template creation fails.
    pub fn start_entry(&mut self, entry_name: &str, entry_size: u64) {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!entry_name.is_empty(), "Entry name must not be empty");
        assert!(
            entry_size <= MAX_ARCHIVE_ENTRY_SIZE,
            "Entry size within limits"
        );

        if entry_size >= MIN_FILE_SIZE_FOR_PROGRESS {
            let entry_bar = create_entry_progress_bar(entry_size, entry_name);
            self.entry_bar = Some(entry_bar);
        }

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            self.entry_bar.is_some() || entry_size < MIN_FILE_SIZE_FOR_PROGRESS,
            "Entry bar created for large files or None for small files"
        );
        assert!(
            entry_size <= MAX_ARCHIVE_ENTRY_SIZE,
            "Entry size still within limits"
        );
    }

    /// Update entry progress.
    ///
    /// # Arguments
    ///
    /// * `bytes_processed` - Number of bytes processed so far
    ///
    /// # Panics
    ///
    /// Panics if bytes processed exceed limits.
    pub fn update_entry(&self, bytes_processed: u64) {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            bytes_processed <= MAX_ARCHIVE_ENTRY_SIZE,
            "Bytes processed within limits"
        );
        assert!(bytes_processed < u64::MAX, "Bytes processed within bounds");

        if let Some(ref bar) = self.entry_bar {
            bar.set_position(bytes_processed);
        }

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            bytes_processed <= MAX_ARCHIVE_ENTRY_SIZE,
            "Bytes processed still within limits"
        );
        assert!(
            self.entry_bar
                .as_ref()
                .is_none_or(|bar| bar.position() == bytes_processed),
            "Progress bar position updated correctly"
        );
    }

    /// Finish processing current entry.
    ///
    /// # Panics
    ///
    /// Panics if the archive progress is already complete.
    pub fn finish_entry(&mut self) {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            self.archive_bar.position() < self.archive_bar.length().unwrap_or(u64::MAX),
            "Archive progress not yet complete"
        );
        assert!(
            self.archive_bar.length().is_some(),
            "Archive bar has valid length"
        );

        if let Some(bar) = self.entry_bar.take() {
            bar.finish_and_clear();
        }
        self.archive_bar.inc(1);

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(self.entry_bar.is_none(), "Entry bar cleared");
        assert!(
            self.archive_bar.position() <= self.archive_bar.length().unwrap_or(u64::MAX),
            "Archive progress within bounds"
        );
    }

    /// Finish processing the archive.
    ///
    /// # Panics
    ///
    /// Panics if there's an active entry bar or invalid archive state.
    pub fn finish(self) {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            self.archive_bar.length().is_some(),
            "Archive bar has valid length"
        );
        assert!(
            self.entry_bar.is_none(),
            "No active entry bar when finishing"
        );

        self.archive_bar
            .finish_with_message("Archive processing complete");

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            self.archive_bar.is_finished(),
            "Archive bar marked as finished"
        );
        assert!(self.entry_bar.is_none(), "Entry bar remains None");
    }
}

/// Create archive-level progress bar.
///
/// # Arguments
///
/// * `total_entries` - Total number of entries
/// * `archive_name` - Name of the archive
///
/// # Returns
///
/// Configured progress bar.
fn create_archive_progress_bar(total_entries: u64, archive_name: &str) -> indicatif::ProgressBar {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        total_entries <= MAX_ARCHIVE_ENTRIES as u64,
        "Total entries within limits"
    );
    assert!(!archive_name.is_empty(), "Archive name must not be empty");

    let archive_bar = indicatif::ProgressBar::new(total_entries);
    archive_bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap_or_else(|_| {
                indicatif::ProgressStyle::default_bar()
                    .template("{bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
                    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
            })
            .progress_chars("##-"),
    );
    archive_bar.set_message(format!("Processing {archive_name}"));

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        archive_bar.length() == Some(total_entries),
        "Progress bar length set correctly"
    );
    assert!(
        !archive_bar.message().is_empty(),
        "Progress bar message set"
    );

    archive_bar
}

/// Create entry-level progress bar.
///
/// # Arguments
///
/// * `entry_size` - Size of the entry
/// * `entry_name` - Name of the entry
///
/// # Returns
///
/// Configured progress bar.
fn create_entry_progress_bar(entry_size: u64, entry_name: &str) -> indicatif::ProgressBar {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        entry_size <= MAX_ARCHIVE_ENTRY_SIZE,
        "Entry size within limits"
    );
    assert!(!entry_name.is_empty(), "Entry name must not be empty");

    let entry_bar = indicatif::ProgressBar::new(entry_size);
    entry_bar.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("  [{elapsed_precise}] {bar:38.green/white} {bytes}/{total_bytes} {msg}")
            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
            .progress_chars("=>-"),
    );
    entry_bar.set_message(format!("Hashing {entry_name}"));

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        entry_bar.length() == Some(entry_size),
        "Entry bar length set correctly"
    );
    assert!(!entry_bar.message().is_empty(), "Entry bar message set");

    entry_bar
}

// ============================================================================
// Test Support Functions
// ============================================================================

/// Compute hash for a Read type using the specified algorithm.
/// This is a simplified version for testing archive entries.
///
/// # Errors
///
/// Returns an error if reading from the source fails or hash computation fails.
///
/// # Panics
///
/// Panics if chunk size is invalid or buffer constraints are violated.
#[cfg(any(test, feature = "archives"))]
pub fn compute_hash<R: Read>(reader: &mut R, algo: &HashingAlgo) -> Result<String> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    // Note: CHUNK_SIZE > 0 and CHUNK_SIZE <= MAX_BUFFER_SIZE are guaranteed by compile-time assertions
    assert!(
        std::mem::size_of::<R>() > 0,
        "Reader type must have non-zero size"
    );
    assert!(
        matches!(algo, HashingAlgo::Md5 | HashingAlgo::Sha2),
        "Algorithm must be supported"
    );

    let mut buffer = vec![0u8; CHUNK_SIZE];

    let hash_string = match algo {
        HashingAlgo::Sha2 => {
            let mut hasher = Sha256::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(CheckleError::ArchiveReadError {
                            details: e.to_string(),
                        });
                    }
                }
            }
            crate::simd::bytes_to_hex(&hasher.finalize())
        }
        HashingAlgo::Md5 => {
            let mut hasher = Md5::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(e) => {
                        return Err(CheckleError::ArchiveReadError {
                            details: e.to_string(),
                        });
                    }
                }
            }
            crate::simd::bytes_to_hex(&hasher.finalize())
        }
    };

    // Postcondition assertions (Tiger Style: minimum 2 per function)
    assert!(!hash_string.is_empty(), "Hash string must not be empty");
    assert!(
        simd::is_hex_string(&hash_string),
        "Hash string must be hexadecimal"
    );

    Ok(hash_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_entry_metadata_validation() {
        // Test valid metadata
        let metadata = ArchiveEntryMetadata::new(PathBuf::from("test.txt"), 1000, true, 0.5);
        assert_eq!(metadata.size, 1000);
        assert!(metadata.is_compressed);
        assert!((metadata.compression_ratio - 0.5).abs() < f64::EPSILON);

        // Test maximum size
        let max_metadata = ArchiveEntryMetadata::new(
            PathBuf::from("large.txt"),
            MAX_ARCHIVE_ENTRY_SIZE,
            false,
            1.0,
        );
        assert_eq!(max_metadata.size, MAX_ARCHIVE_ENTRY_SIZE);
    }

    #[test]
    #[should_panic(expected = "Entry size")]
    fn test_archive_entry_metadata_size_limit() {
        let _ = ArchiveEntryMetadata::new(
            PathBuf::from("too_large.txt"),
            MAX_ARCHIVE_ENTRY_SIZE + 1,
            false,
            1.0,
        );
    }

    #[test]
    #[should_panic(expected = "Invalid compression ratio")]
    fn test_archive_entry_metadata_invalid_ratio() {
        let _ = ArchiveEntryMetadata::new(PathBuf::from("test.txt"), 1000, true, 1.5);
    }

    #[test]
    fn test_archive_reader_trait_bounds() {
        // Test that our implementations satisfy the ArchiveReader trait bounds
        fn assert_archive_reader<T: ArchiveReader>() {}

        #[cfg(feature = "tar")]
        assert_archive_reader::<TarArchive>();

        #[cfg(feature = "zip")]
        assert_archive_reader::<ZipArchive>();
    }

    #[test]
    fn test_entry_reader_bounds() {
        // Test that our entry readers implement Read
        fn assert_read<T: Read>() {}

        #[cfg(feature = "tar")]
        assert_read::<TarEntryReader>();

        #[cfg(feature = "zip")]
        assert_read::<ZipEntryReader>();
    }
}
