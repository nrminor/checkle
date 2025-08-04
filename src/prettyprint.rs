//! Pretty table output formatting for checkle commands.
//!
//! This module provides enhanced output formatting for various checkle operations
//! including hash generation and verification results, displaying them in formatted
//! tables to stderr for improved readability. The implementation supports multiple
//! data types through a generic trait system while maintaining backward compatibility.

use jiff::Timestamp;
use prettytable::{Attr, Cell, Row, Table, color};
use std::{
    fs::Metadata,
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::{
    constants::{
        EXPECTED_PERMISSION_STRING_LENGTH, HASH_TRUNCATE_CHARS, MAX_ERROR_MESSAGE_LENGTH,
        MAX_FILE_SIZE_PRETTY as MAX_FILE_SIZE, MAX_HASH_DISPLAY_LENGTH,
        MAX_PAIRS_COUNT_PRETTY as MAX_PAIRS_COUNT, MAX_PATH_DISPLAY_LENGTH,
        MAX_STRING_LENGTH_PRETTY as MAX_STRING_LENGTH, MAX_TIMESTAMP_PRETTY as MAX_TIMESTAMP,
        MIN_TRUNCATE_LENGTH, PATH_TRUNCATE_CHARS, THRESHOLD, UNITS, VALID_PERMISSION_MASK,
    },
    errors::{CheckleError, EmptyHash, InvalidHashFormat, StderrWriteError},
};

/// Trait for types that can be displayed as rows in a pretty table.
///
/// This trait defines the interface for converting data structures into
/// tabular format suitable for display in formatted tables. Implementing
/// types provide column definitions and row formatting logic.
pub trait PrettyTableRow {
    /// Returns the column headers for this table type.
    ///
    /// The headers define the column structure and should remain consistent
    /// across all instances of the implementing type.
    fn column_headers() -> Vec<&'static str>;

    /// Formats this instance as a table row.
    ///
    /// Returns a vector of strings representing the values for each column,
    /// in the same order as defined by `column_headers()`.
    fn format_row(&self) -> Vec<String>;

    /// Returns the title for this table type.
    ///
    /// Used as the table heading when displaying multiple rows.
    fn table_title() -> &'static str;
}

/// Status of a verification operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationStatus {
    /// File verification passed - hash matches
    Pass,
    /// File verification failed - hash mismatch
    Fail,
    /// File is missing or inaccessible
    Missing,
    /// Error occurred during verification
    Error(String),
}

impl VerificationStatus {
    /// Returns true if this status represents a successful verification.
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, VerificationStatus::Pass)
    }

    /// Returns true if this status represents a failure.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(self, VerificationStatus::Fail)
    }

    /// Returns true if this status represents an error condition.
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            VerificationStatus::Missing | VerificationStatus::Error(_)
        )
    }

    /// Returns the display string for this status.
    #[must_use]
    pub fn display_string(&self) -> String {
        match self {
            VerificationStatus::Pass => "PASS".to_string(),
            VerificationStatus::Fail => "FAIL".to_string(),
            VerificationStatus::Missing => "MISSING".to_string(),
            VerificationStatus::Error(msg) => format!("ERROR: {msg}"),
        }
    }

    /// Returns the symbol for this status.
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            VerificationStatus::Pass => "✓",
            VerificationStatus::Fail => "✗",
            VerificationStatus::Missing | VerificationStatus::Error(_) => "⚠",
        }
    }
}

/// Result of a hash verification operation.
///
/// This struct contains information about a file verification including
/// the expected hash, actual computed hash, and verification status.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// The file path that was verified
    file: std::path::PathBuf,
    /// The expected hash value
    expected_hash: String,
    /// The actual computed hash value (empty for missing/error cases)
    actual_hash: String,
    /// The verification status
    status: VerificationStatus,
    /// Optional error message for additional context
    error_message: Option<String>,
    /// File size in bytes (if available)
    file_size: Option<u64>,
    /// File modification time (if available)
    modified_time: Option<std::time::SystemTime>,
}

impl VerificationResult {
    /// Creates a new verification result for a successful or failed verification.
    ///
    /// # Arguments
    /// * `file` - The file path that was verified
    /// * `expected_hash` - The expected hash value
    /// * `actual_hash` - The actual computed hash value
    /// * `passed` - Whether the verification passed
    ///
    /// # Returns
    /// A new `VerificationResult` instance.
    ///
    /// # Errors
    /// Returns an error if:
    /// - Either hash is empty
    /// - Either hash contains non-hexadecimal characters
    /// - Either hash exceeds maximum allowed length
    ///
    /// # Panics
    /// Panics if either hash length exceeds the system maximum string length.
    pub fn new(
        file: std::path::PathBuf,
        expected_hash: String,
        actual_hash: String,
        passed: bool,
    ) -> Result<Self, CheckleError> {
        // Validate expected hash
        if expected_hash.is_empty() {
            return Err(EmptyHash);
        }
        if !expected_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidHashFormat {
                hash: expected_hash,
            });
        }

        // Validate actual hash
        if actual_hash.is_empty() {
            return Err(EmptyHash);
        }
        if !actual_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidHashFormat { hash: actual_hash });
        }

        // True invariants that indicate system limits
        assert!(
            expected_hash.len() <= MAX_STRING_LENGTH,
            "Expected hash length {} exceeds system maximum {}",
            expected_hash.len(),
            MAX_STRING_LENGTH
        );
        assert!(
            actual_hash.len() <= MAX_STRING_LENGTH,
            "Actual hash length {} exceeds system maximum {}",
            actual_hash.len(),
            MAX_STRING_LENGTH
        );

        let status = if passed {
            VerificationStatus::Pass
        } else {
            VerificationStatus::Fail
        };

        Ok(Self {
            file,
            expected_hash,
            actual_hash,
            status,
            error_message: None,
            file_size: None,
            modified_time: None,
        })
    }

    /// Creates a new verification result with file metadata.
    ///
    /// # Arguments
    /// * `file` - The file path that was verified
    /// * `expected_hash` - The expected hash value
    /// * `actual_hash` - The actual computed hash value
    /// * `passed` - Whether the verification passed
    /// * `metadata` - File metadata containing size and modification time
    ///
    /// # Returns
    /// A new `VerificationResult` instance with metadata.
    ///
    /// # Errors
    /// Returns an error if hash validation fails (see [`new`])
    pub fn new_with_metadata(
        file: std::path::PathBuf,
        expected_hash: String,
        actual_hash: String,
        passed: bool,
        metadata: &std::fs::Metadata,
    ) -> Result<Self, CheckleError> {
        let mut result = Self::new(file, expected_hash, actual_hash, passed)?;
        result.file_size = Some(metadata.len());
        result.modified_time = metadata.modified().ok();
        Ok(result)
    }

    /// Creates a new verification result for a missing file.
    ///
    /// # Arguments
    /// * `file` - The file path that was expected to be verified
    /// * `expected_hash` - The expected hash value
    ///
    /// # Returns
    /// A new `VerificationResult` instance with Missing status.
    #[must_use]
    pub fn new_missing(file: std::path::PathBuf, expected_hash: String) -> Self {
        debug_assert!(
            expected_hash.len() <= MAX_STRING_LENGTH,
            "Expected hash length exceeds maximum allowed"
        );

        Self {
            file,
            expected_hash,
            actual_hash: String::new(),
            status: VerificationStatus::Missing,
            error_message: None,
            file_size: None,
            modified_time: None,
        }
    }

    /// Creates a new verification result for an error condition.
    ///
    /// # Arguments
    /// * `file` - The file path that was attempted to be verified
    /// * `expected_hash` - The expected hash value
    /// * `error_message` - The error that occurred
    ///
    /// # Returns
    /// A new `VerificationResult` instance with Error status.
    #[must_use]
    pub fn new_error(
        file: std::path::PathBuf,
        expected_hash: String,
        error_message: String,
    ) -> Self {
        debug_assert!(
            expected_hash.len() <= MAX_STRING_LENGTH,
            "Expected hash length exceeds maximum allowed"
        );
        debug_assert!(
            error_message.len() <= MAX_STRING_LENGTH,
            "Error message length exceeds maximum allowed"
        );

        Self {
            file,
            expected_hash,
            actual_hash: String::new(),
            status: VerificationStatus::Error(error_message.clone()),
            error_message: Some(error_message),
            file_size: None,
            modified_time: None,
        }
    }

    /// Returns a reference to the file path.
    #[must_use]
    pub fn file(&self) -> &std::path::Path {
        &self.file
    }

    /// Returns a reference to the expected hash.
    #[must_use]
    pub fn expected_hash(&self) -> &str {
        &self.expected_hash
    }

    /// Returns a reference to the actual hash.
    #[must_use]
    pub fn actual_hash(&self) -> &str {
        &self.actual_hash
    }

    /// Returns the verification status.
    #[must_use]
    pub fn status(&self) -> &VerificationStatus {
        &self.status
    }

    /// Returns whether the verification passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.status.is_success()
    }

    /// Returns the error message, if any.
    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Returns the file size, if available.
    #[must_use]
    pub fn file_size(&self) -> Option<u64> {
        self.file_size
    }

    /// Returns the modified time, if available.
    #[must_use]
    pub fn modified_time(&self) -> Option<std::time::SystemTime> {
        self.modified_time
    }
}

impl PrettyTableRow for VerificationResult {
    fn column_headers() -> Vec<&'static str> {
        vec![
            "File Path",
            "Status",
            "Expected Hash",
            "Computed Hash",
            "Size",
            "Modified",
            "Error Message",
        ]
    }

    fn format_row(&self) -> Vec<String> {
        // Precondition: Validate expected hash exists
        assert!(
            !self.expected_hash.is_empty(),
            "Expected hash must not be empty"
        );

        let file_path_display = format_path_display(&self.file);
        let status_display = self.format_status_display();
        let expected_display = format_hash_display(&self.expected_hash);
        let actual_display = self.format_actual_hash_display();
        let error_display = self.format_error_display();
        let size_display = self
            .file_size
            .map_or_else(|| "-".to_string(), format_file_size);
        let modified_display = self
            .modified_time
            .map_or_else(|| "-".to_string(), format_datetime_from_system_time);

        let result = vec![
            file_path_display,
            status_display,
            expected_display,
            actual_display,
            size_display,
            modified_display,
            error_display,
        ];

        // Postcondition: Validate result structure
        assert_eq!(
            result.len(),
            7,
            "Verification result must have exactly 7 columns"
        );
        assert!(
            result.iter().all(|s| !s.is_empty()),
            "All columns must be non-empty"
        );

        result
    }

    fn table_title() -> &'static str {
        "Verification Results"
    }
}

// Helper methods to keep format_row under 70 lines
impl VerificationResult {
    /// Formats the status display with symbol and text
    fn format_status_display(&self) -> String {
        match &self.status {
            VerificationStatus::Pass => {
                let symbol = self.status.symbol();
                format!("{symbol} PASS")
            }
            VerificationStatus::Fail => {
                let symbol = self.status.symbol();
                format!("{symbol} FAIL")
            }
            VerificationStatus::Missing => {
                let symbol = self.status.symbol();
                format!("{symbol} MISS")
            }
            VerificationStatus::Error(_) => {
                let symbol = self.status.symbol();
                format!("{symbol} ERR")
            }
        }
    }

    /// Formats the actual hash display
    fn format_actual_hash_display(&self) -> String {
        if self.actual_hash.is_empty() {
            "-".to_string()
        } else {
            format_hash_display(&self.actual_hash)
        }
    }

    /// Formats the error message display
    fn format_error_display(&self) -> String {
        match &self.status {
            VerificationStatus::Error(msg) => truncate_string(msg, MAX_ERROR_MESSAGE_LENGTH),
            VerificationStatus::Missing => "File not found".to_string(),
            _ => "-".to_string(),
        }
    }
}

/// Enhanced file-hash pair that includes metadata for pretty printing.
///
/// This struct extends the basic hash-file pairing with filesystem metadata
/// to enable rich table output while maintaining compatibility with existing code.
#[derive(Debug, Clone)]
pub struct FileHashPairWithMetadata {
    /// The file path
    file: PathBuf,
    /// The computed hash
    hash: String,
    /// File size in bytes
    file_size: u64,
    /// Last modified time as Unix timestamp
    modified_time: Option<u64>,
    /// File extension (without the dot)
    file_extension: Option<String>,
    /// File permissions as Unix mode bits
    permissions: Option<u32>,
}

impl FileHashPairWithMetadata {
    /// Creates a new enhanced file-hash pair with metadata.
    ///
    /// # Arguments
    /// * `file` - The file path
    /// * `hash` - The computed hash string
    /// * `metadata` - Filesystem metadata for the file
    ///
    /// # Returns
    /// A new `FileHashPairWithMetadata` instance with extracted metadata.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The hash is empty
    /// - The hash contains non-hexadecimal characters
    /// - The hash exceeds maximum allowed length
    ///
    /// # Panics
    /// Panics if the hash length or file size exceeds system maximums.
    pub fn new(file: PathBuf, hash: String, metadata: &Metadata) -> Result<Self, CheckleError> {
        // Validate hash
        if hash.is_empty() {
            return Err(EmptyHash);
        }
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidHashFormat { hash });
        }

        // True invariants for system limits
        assert!(
            hash.len() <= MAX_STRING_LENGTH,
            "Hash length {} exceeds system maximum {}",
            hash.len(),
            MAX_STRING_LENGTH
        );

        // Resource limits validation
        let file_size = metadata.len();
        assert!(
            file_size <= MAX_FILE_SIZE,
            "File size {file_size} exceeds system maximum {MAX_FILE_SIZE}"
        );

        // Extract components using helper functions to keep this method under 70 lines
        let modified_time = Self::extract_modified_time(metadata);
        let file_extension = Self::extract_file_extension(&file);
        let permissions = Self::extract_permissions(metadata);

        let result = Self {
            file,
            hash,
            file_size,
            modified_time,
            file_extension,
            permissions,
        };

        // Postcondition invariant
        assert!(
            result.file_size <= MAX_FILE_SIZE,
            "Result file size exceeds maximum"
        );

        Ok(result)
    }

    /// Creates a new enhanced file-hash pair with fallback metadata.
    ///
    /// This method is used when metadata collection fails, providing
    /// sensible defaults to avoid breaking the hashing pipeline.
    ///
    /// # Arguments
    /// * `file` - The file path
    /// * `hash` - The computed hash string
    ///
    /// # Returns
    /// A new `FileHashPairWithMetadata` instance with minimal metadata.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The hash is empty
    /// - The hash contains non-hexadecimal characters
    ///
    /// # Panics
    /// Panics if the hash length exceeds the system maximum.
    pub fn new_with_fallback(file: PathBuf, hash: String) -> Result<Self, CheckleError> {
        // Validate hash
        if hash.is_empty() {
            return Err(EmptyHash);
        }
        if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidHashFormat { hash });
        }

        // True invariant for system limit
        assert!(
            hash.len() <= MAX_STRING_LENGTH,
            "Hash length {} exceeds system maximum {}",
            hash.len(),
            MAX_STRING_LENGTH
        );

        // Extract file extension using helper function
        let file_extension = Self::extract_file_extension(&file);

        Ok(Self {
            file,
            hash,
            file_size: 0,
            modified_time: None,
            file_extension,
            permissions: None,
        })
    }

    /// Returns a reference to the file path.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Returns a reference to the hash string.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Returns the file extension, if available.
    #[must_use]
    pub fn file_extension(&self) -> Option<&str> {
        self.file_extension.as_deref()
    }

    /// Returns the last modified time as Unix timestamp, if available.
    #[must_use]
    pub fn modified_time(&self) -> Option<u64> {
        self.modified_time
    }

    /// Returns the file permissions as Unix mode bits, if available.
    #[must_use]
    pub fn permissions(&self) -> Option<u32> {
        self.permissions
    }

    /// Converts this enhanced pair into a regular file-hash pair.
    ///
    /// This provides backward compatibility with existing code that expects
    /// the basic `FileHashPair` type.
    #[must_use]
    pub fn into_basic_pair(self) -> crate::io::FileHashPair {
        // No need for validation - hash was already validated at construction
        // Use new_unchecked since archive paths don't exist as filesystem files
        crate::io::FileHashPair::new_unchecked(self.file, self.hash)
    }

    // Tiger Style: Helper functions to keep main methods under 70 lines

    /// Extracts modified time from metadata.
    fn extract_modified_time(metadata: &Metadata) -> Option<u64> {
        // Tiger Style: Precondition assertion
        debug_assert!(
            metadata.len() <= MAX_FILE_SIZE,
            "Metadata file size exceeds maximum"
        );

        let result = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .filter(|&timestamp| timestamp <= MAX_TIMESTAMP);

        // Tiger Style: Postcondition assertion
        if let Some(timestamp) = result {
            debug_assert!(
                timestamp <= MAX_TIMESTAMP,
                "Extracted timestamp {timestamp} exceeds maximum allowed"
            );
        }

        result
    }

    /// Extracts file extension from path.
    fn extract_file_extension(file: &Path) -> Option<String> {
        // Tiger Style: Precondition assertion
        debug_assert!(!file.as_os_str().is_empty(), "File path must not be empty");

        let result = file
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .filter(|ext| ext.len() <= MAX_STRING_LENGTH);

        // Tiger Style: Postcondition assertion
        if let Some(ref ext) = result {
            debug_assert!(
                ext.len() <= MAX_STRING_LENGTH,
                "Extension length exceeds maximum"
            );
            debug_assert!(!ext.is_empty(), "Extension must not be empty if present");
        }

        result
    }

    /// Extracts Unix permissions from metadata.
    fn extract_permissions(metadata: &Metadata) -> Option<u32> {
        // Tiger Style: Precondition assertion
        debug_assert!(
            metadata.len() <= MAX_FILE_SIZE,
            "Metadata file size exceeds maximum"
        );

        #[cfg(unix)]
        let result = {
            let mode = metadata.permissions().mode();
            Some(mode & VALID_PERMISSION_MASK)
        };

        #[cfg(not(unix))]
        let result = None;

        // Tiger Style: Postcondition assertions
        if let Some(perms) = result {
            debug_assert!(
                perms <= VALID_PERMISSION_MASK,
                "Extracted permissions {perms} exceed valid mask"
            );
        }

        result
    }
}

impl PrettyTableRow for FileHashPairWithMetadata {
    fn column_headers() -> Vec<&'static str> {
        vec![
            "Hash",
            "File Path",
            "Size",
            "Modified",
            "Extension",
            "Permissions",
        ]
    }

    fn format_row(&self) -> Vec<String> {
        // Precondition: Validate inputs
        assert!(!self.hash().is_empty(), "Hash must not be empty");
        assert!(
            self.file_size() <= MAX_FILE_SIZE,
            "File size {} exceeds maximum {}",
            self.file_size(),
            MAX_FILE_SIZE
        );

        let hash_display = format_hash_display(self.hash());
        let file_path_display = format_path_display(self.file());
        let size_display = format_file_size(self.file_size());
        let modified_display = self
            .modified_time()
            .map_or_else(|| "Unknown".to_string(), format_datetime);
        let extension_display = self
            .file_extension()
            .map_or_else(|| "-".to_string(), std::string::ToString::to_string);
        let permissions_display = self
            .permissions()
            .map_or_else(|| "Unknown".to_string(), format_permissions);

        let result = vec![
            hash_display,
            file_path_display,
            size_display,
            modified_display,
            extension_display,
            permissions_display,
        ];

        // Postcondition: Validate result structure
        assert_eq!(result.len(), 6, "Hash result must have exactly 6 columns");
        assert!(
            result.iter().all(|s| !s.is_empty()),
            "All columns must be non-empty"
        );

        result
    }

    fn table_title() -> &'static str {
        "Hash Results"
    }
}

/// Formats file size in human-readable format (B, KB, MB, GB, TB).
///
/// # Arguments
/// * `size` - File size in bytes
///
/// # Returns
/// A formatted string representing the file size with appropriate units.
///
/// # Examples
/// ```rust
/// use checkle::prettyprint::format_file_size;
///
/// assert_eq!(format_file_size(0), "0 B");
/// assert_eq!(format_file_size(1024), "1.00 KB");
/// assert_eq!(format_file_size(1536), "1.50 KB");
/// assert_eq!(format_file_size(1048576), "1.00 MB");
/// ```
#[must_use]
pub fn format_file_size(size: u64) -> String {
    // Tiger Style: Precondition assertions
    debug_assert!(
        size <= MAX_FILE_SIZE,
        "File size {size} exceeds maximum allowed {MAX_FILE_SIZE}"
    );

    if size == 0 {
        let result = "0 B".to_string();
        // Tiger Style: Postcondition assertion
        debug_assert!(!result.is_empty(), "Result must not be empty");
        debug_assert!(
            result.len() <= MAX_STRING_LENGTH,
            "Result length {} exceeds maximum {MAX_STRING_LENGTH}",
            result.len()
        );
        return result;
    }

    #[allow(clippy::cast_precision_loss)]
    let size_f = size as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let unit_index = (size_f.log2() / THRESHOLD.log2()).floor() as usize;
    let unit_index = unit_index.min(UNITS.len() - 1);

    // Tiger Style: Bounds checking
    debug_assert!(
        unit_index < UNITS.len(),
        "Unit index {unit_index} exceeds UNITS array length"
    );

    let result = if unit_index == 0 {
        format!("{size} B")
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let adjusted_size = size_f / THRESHOLD.powi(unit_index as i32);
        // Tiger Style: Value range assertion
        debug_assert!(adjusted_size > 0.0, "Adjusted size must be positive");

        if adjusted_size >= 100.0 {
            let unit = UNITS[unit_index];
            format!("{adjusted_size:.0} {unit}")
        } else if adjusted_size >= 10.0 {
            let unit = UNITS[unit_index];
            format!("{adjusted_size:.1} {unit}")
        } else {
            let unit = UNITS[unit_index];
            format!("{adjusted_size:.2} {unit}")
        }
    };

    // Tiger Style: Postcondition assertions
    debug_assert!(!result.is_empty(), "Result must not be empty");
    debug_assert!(
        result.len() <= MAX_STRING_LENGTH,
        "Result length {} exceeds maximum {MAX_STRING_LENGTH}",
        result.len()
    );
    debug_assert!(
        result.chars().any(char::is_alphabetic),
        "Result must contain unit designation"
    );

    result
}

/// Formats Unix timestamp as human-readable datetime string.
///
/// # Arguments
/// * `timestamp` - Unix timestamp in seconds
///
/// # Returns
/// A formatted datetime string in "YYYY-MM-DD HH:MM:SS" format, or "Unknown" if conversion fails.
///
/// # Examples
/// ```rust
/// use checkle::prettyprint::format_datetime;
///
/// let result = format_datetime(1609459200); // 2021-01-01 00:00:00 UTC
/// // Result will be an absolute timestamp like "2021-01-01 00:00:00"
/// assert!(!result.is_empty());
/// ```
#[must_use]
pub fn format_datetime(timestamp: u64) -> String {
    // Tiger Style: Precondition assertions
    debug_assert!(
        timestamp <= MAX_TIMESTAMP,
        "Timestamp {timestamp} exceeds maximum allowed {MAX_TIMESTAMP}"
    );

    // Convert Unix timestamp to jiff Timestamp and format
    #[allow(clippy::cast_possible_wrap)]
    let result = match Timestamp::from_second(timestamp as i64) {
        Ok(ts) => {
            // Format as "YYYY-MM-DD HH:MM:SS" in local time
            ts.strftime("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => "Unknown".to_string(),
    };

    // Tiger Style: Postcondition assertions
    debug_assert!(!result.is_empty(), "Result must not be empty");
    debug_assert!(
        result.len() <= MAX_STRING_LENGTH,
        "Result length {} exceeds maximum {MAX_STRING_LENGTH}",
        result.len()
    );

    result
}

/// Tiger Style: Helper function to keep `format_datetime` under 70 lines
fn format_datetime_from_system_time(datetime: std::time::SystemTime) -> String {
    // Convert SystemTime to jiff Timestamp and format as absolute datetime
    match Timestamp::try_from(datetime) {
        Ok(ts) => {
            // Format as "YYYY-MM-DD HH:MM:SS" in local time
            // Using strftime for consistent formatting
            ts.strftime("%Y-%m-%d %H:%M:%S").to_string()
        }
        Err(_) => "Unknown".to_string(),
    }
}

/// Formats Unix file permissions as a human-readable string.
///
/// # Arguments
/// * `mode` - Unix file mode bits
///
/// # Returns
/// A formatted permission string (e.g., "rw-r--r--") or "Unknown" if invalid.
///
/// # Examples
/// ```rust
/// use checkle::prettyprint::format_permissions;
///
/// assert_eq!(format_permissions(0o644), "rw-r--r--");
/// assert_eq!(format_permissions(0o755), "rwxr-xr-x");
/// ```
#[must_use]
pub fn format_permissions(mode: u32) -> String {
    // Tiger Style: Precondition assertions
    debug_assert!(
        mode <= VALID_PERMISSION_MASK,
        "Mode {mode} exceeds valid permission mask {VALID_PERMISSION_MASK}"
    );
    debug_assert!(
        (mode & !VALID_PERMISSION_MASK) == 0,
        "Mode contains invalid permission bits"
    );

    let mut perms = String::with_capacity(EXPECTED_PERMISSION_STRING_LENGTH);

    // Owner permissions
    perms.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o100 != 0 { 'x' } else { '-' });

    // Group permissions
    perms.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o010 != 0 { 'x' } else { '-' });

    // Other permissions
    perms.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    perms.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    perms.push(if mode & 0o001 != 0 { 'x' } else { '-' });

    // Tiger Style: Postcondition assertions
    debug_assert!(
        perms.len() == EXPECTED_PERMISSION_STRING_LENGTH,
        "Permission string length {} is not expected {EXPECTED_PERMISSION_STRING_LENGTH}",
        perms.len()
    );
    debug_assert!(
        perms
            .chars()
            .all(|c| c == 'r' || c == 'w' || c == 'x' || c == '-'),
        "Permission string contains invalid characters"
    );
    debug_assert!(!perms.is_empty(), "Permission string must not be empty");

    perms
}

/// Displays verification results in a pretty table with colored output.
///
/// This function creates a formatted table specifically for verification results
/// with colored status indicators: green for PASS, red for FAIL, yellow for MISSING/ERROR.
///
/// # Arguments
/// * `results` - Collection of verification results to display
///
/// # Errors
/// Returns an error if table formatting fails or if writing to stderr fails.
///
/// # Panics
/// Panics if the results count exceeds the maximum allowed.
pub fn display_verification_table(results: &[VerificationResult]) -> Result<(), CheckleError> {
    // Precondition: Check resource limits
    assert!(
        results.len() <= MAX_PAIRS_COUNT,
        "Results count {} exceeds maximum allowed {}",
        results.len(),
        MAX_PAIRS_COUNT
    );

    if results.is_empty() {
        return Ok(());
    }

    let mut table = create_table_with_generic_headers::<VerificationResult>();
    add_colored_verification_rows(&mut table, results);
    print_generic_table_to_stderr::<VerificationResult>(&table)?;

    // Postcondition: Table structure invariant
    assert!(!table.is_empty(), "Table must have at least header row");

    Ok(())
}

/// Displays verification results with a summary at the end.
///
/// This function displays the verification table followed by a summary
/// showing counts of passed, failed, missing, and error results.
///
/// # Arguments
/// * `results` - Collection of verification results to display
///
/// # Errors
/// Returns an error if table formatting fails or if writing to stderr fails.
///
/// # Panics
/// Panics if the results count exceeds the maximum allowed.
pub fn display_verification_table_with_summary(
    results: &[VerificationResult],
) -> Result<(), CheckleError> {
    // Precondition: Check resource limits
    assert!(
        results.len() <= MAX_PAIRS_COUNT,
        "Results count {} exceeds maximum allowed {}",
        results.len(),
        MAX_PAIRS_COUNT
    );

    // Display the main table
    display_verification_table(results)?;

    if !results.is_empty() {
        // Calculate summary statistics
        let summary = VerificationSummary::from_results(results);

        // Print summary to stderr
        let mut stderr = io::stderr();
        writeln!(
            stderr,
            "Summary: {}/{} passed, {} failed, {} missing, {} errors",
            summary.passed, summary.total, summary.failed, summary.missing, summary.errors
        )
        .map_err(|e| StderrWriteError {
            details: "Failed to write verification summary".to_string(),
            source: e,
        })?;

        writeln!(stderr).map_err(|e| StderrWriteError {
            details: "Failed to write final newline".to_string(),
            source: e,
        })?;
    }

    Ok(())
}

/// Helper function to add colored hash rows to table
fn add_colored_hash_rows(table: &mut Table, pairs: &[FileHashPairWithMetadata]) {
    // Tiger Style: Precondition assertions
    debug_assert!(
        pairs.len() <= MAX_PAIRS_COUNT,
        "Pairs count exceeds maximum"
    );
    debug_assert!(
        !table.is_empty(),
        "Table must have header row before adding data"
    );

    for pair in pairs {
        let row_data = pair.format_row();

        // Create cells with appropriate coloring for different columns
        let mut cells: Vec<Cell> = Vec::with_capacity(row_data.len());

        for (index, data) in row_data.iter().enumerate() {
            let cell = if should_use_colors() {
                match index {
                    0 => {
                        // Hash column - display in cyan for better readability
                        Cell::new(data).with_style(Attr::ForegroundColor(color::CYAN))
                    }
                    4 => {
                        // Extension column - color by file type
                        let color_attr = match data.to_lowercase().as_str() {
                            "txt" | "md" | "rst" => color::GREEN,
                            "fasta" | "fa" | "fastq" | "fq" => color::BLUE,
                            "bam" | "sam" | "vcf" | "bcf" => color::MAGENTA,
                            "json" | "yaml" | "yml" | "xml" => color::YELLOW,
                            "bin" | "exe" | "so" | "dll" => color::RED,
                            _ => color::WHITE,
                        };
                        Cell::new(data).with_style(Attr::ForegroundColor(color_attr))
                    }
                    _ => Cell::new(data),
                }
            } else {
                Cell::new(data)
            };
            cells.push(cell);
        }

        table.add_row(Row::new(cells));
    }

    // Tiger Style: Postcondition assertion
    debug_assert!(
        table.len() == pairs.len() + 1,
        "Table should have header plus data rows"
    );
}

/// Helper function to add colored verification rows to table
fn add_colored_verification_rows(table: &mut Table, results: &[VerificationResult]) {
    // Tiger Style: Precondition assertions
    debug_assert!(
        results.len() <= MAX_PAIRS_COUNT,
        "Results count exceeds maximum"
    );
    debug_assert!(
        !table.is_empty(),
        "Table must have header row before adding data"
    );

    for result in results {
        let row_data = result.format_row();

        // Create cells with appropriate coloring for status column
        let mut cells: Vec<Cell> = Vec::with_capacity(row_data.len());

        for (index, data) in row_data.iter().enumerate() {
            let cell = if should_use_colors() {
                if index == 1 {
                    // Status column - apply color and bold styling based on verification status
                    match &result.status {
                        VerificationStatus::Pass => Cell::new(data)
                            .with_style(Attr::ForegroundColor(color::GREEN))
                            .with_style(Attr::Bold),
                        VerificationStatus::Fail => Cell::new(data)
                            .with_style(Attr::ForegroundColor(color::RED))
                            .with_style(Attr::Bold),
                        VerificationStatus::Missing | VerificationStatus::Error(_) => {
                            Cell::new(data)
                                .with_style(Attr::ForegroundColor(color::YELLOW))
                                .with_style(Attr::Bold)
                        }
                    }
                } else {
                    // For failed verifications, apply subtle dimming to other columns
                    // This makes the red status stand out more without overwhelming the table
                    match &result.status {
                        VerificationStatus::Pass => Cell::new(data),
                        VerificationStatus::Fail => {
                            // Dim the other columns slightly for failed rows
                            Cell::new(data).with_style(Attr::ForegroundColor(color::BRIGHT_BLACK))
                        }
                        VerificationStatus::Missing | VerificationStatus::Error(_) => {
                            // Keep normal color for missing/error rows since status is already yellow
                            Cell::new(data)
                        }
                    }
                }
            } else {
                Cell::new(data)
            };
            cells.push(cell);
        }

        table.add_row(Row::new(cells));
    }

    // Tiger Style: Postcondition assertion
    debug_assert!(
        table.len() == results.len() + 1,
        "Table should have header plus data rows"
    );
}

/// Creates and displays a pretty table with generic data that implements `PrettyTableRow`.
///
/// This function formats any data type implementing the `PrettyTableRow` trait into
/// a nicely formatted table and prints it to stderr for improved readability.
///
/// # Arguments
/// * `items` - Collection of items implementing `PrettyTableRow`
///
/// # Errors
/// Returns an error if table formatting fails or if writing to stderr fails.
///
/// # Panics
/// Panics if the items count exceeds the maximum allowed.
///
/// # Examples
/// ```rust
/// use checkle::prettyprint::{FileHashPairWithMetadata, display_table};
/// use std::path::PathBuf;
///
/// let pairs = vec![
///     FileHashPairWithMetadata::new_with_fallback(
///         PathBuf::from("test.txt"),
///         "abcdef1234567890".to_string()
///     ).expect("Should create pair")
/// ];
///
/// display_table(&pairs).expect("Should display table");
/// ```
pub fn display_table<T: PrettyTableRow>(items: &[T]) -> Result<(), CheckleError> {
    // Precondition: Check resource limits
    assert!(
        items.len() <= MAX_PAIRS_COUNT,
        "Items count {} exceeds maximum allowed {}",
        items.len(),
        MAX_PAIRS_COUNT
    );

    if items.is_empty() {
        return Ok(());
    }

    let mut table = create_table_with_generic_headers::<T>();
    add_generic_data_rows_to_table(&mut table, items);
    print_generic_table_to_stderr::<T>(&table)?;

    // Postcondition: Table structure invariant
    assert!(!table.is_empty(), "Table must have at least header row");

    Ok(())
}

/// Creates and displays a pretty table with hash results and metadata.
///
/// This function displays hash results with subtle coloring enhancements:
/// - Hash values are displayed in cyan for better readability
/// - File extensions get color coding based on common file types
///
/// # Arguments
/// * `pairs` - Collection of enhanced file-hash pairs with metadata
///
/// # Errors
/// Returns an error if table formatting fails or if writing to stderr fails.
///
/// # Panics
/// Panics if the pairs count exceeds the maximum allowed.
///
/// # Examples
/// ```rust
/// use checkle::prettyprint::{FileHashPairWithMetadata, display_pretty_table};
/// use std::path::PathBuf;
///
/// let pairs = vec![
///     FileHashPairWithMetadata::new_with_fallback(
///         PathBuf::from("test.txt"),
///         "abcdef1234567890".to_string()
///     ).expect("Should create pair")
/// ];
///
/// display_pretty_table(&pairs).expect("Should display table");
/// ```
pub fn display_pretty_table(pairs: &[FileHashPairWithMetadata]) -> Result<(), CheckleError> {
    // Precondition: Check resource limits
    assert!(
        pairs.len() <= MAX_PAIRS_COUNT,
        "Pairs count {} exceeds maximum allowed {}",
        pairs.len(),
        MAX_PAIRS_COUNT
    );

    if pairs.is_empty() {
        return Ok(());
    }

    let mut table = create_table_with_generic_headers::<FileHashPairWithMetadata>();
    add_colored_hash_rows(&mut table, pairs);
    print_generic_table_to_stderr::<FileHashPairWithMetadata>(&table)?;

    // Postcondition: Table structure invariant
    assert!(!table.is_empty(), "Table must have at least header row");

    Ok(())
}

/// Displays verification results in a pretty table with colored output.
///
/// This function creates a formatted table specifically for verification results,
/// with colored status indicators: green for PASS, red for FAIL, yellow for MISSING/ERROR.
///
/// # Arguments
/// * `results` - Collection of verification results
///
/// # Errors
/// Returns an error if table formatting fails or if writing to stderr fails.
///
/// Summary of verification results for display.
#[derive(Debug)]
pub struct VerificationSummary {
    /// Total number of files processed
    pub total: usize,
    /// Number of files that passed verification
    pub passed: usize,
    /// Number of files that failed verification
    pub failed: usize,
    /// Number of files that were missing
    pub missing: usize,
    /// Number of files that had errors
    pub errors: usize,
}

impl VerificationSummary {
    /// Creates a summary from a collection of verification results.
    #[must_use]
    pub fn from_results(results: &[VerificationResult]) -> Self {
        let total = results.len();
        let mut passed = 0;
        let mut failed = 0;
        let mut missing = 0;
        let mut errors = 0;

        for result in results {
            match result.status() {
                VerificationStatus::Pass => passed += 1,
                VerificationStatus::Fail => failed += 1,
                VerificationStatus::Missing => missing += 1,
                VerificationStatus::Error(_) => errors += 1,
            }
        }

        Self {
            total,
            passed,
            failed,
            missing,
            errors,
        }
    }

    /// Displays the summary to stderr.
    ///
    /// # Errors
    /// Returns an error if writing to stderr fails.
    pub fn display(&self) -> Result<(), CheckleError> {
        let mut stderr = io::stderr();

        writeln!(stderr).map_err(|e| StderrWriteError {
            details: "Failed to write blank line".to_string(),
            source: e,
        })?;

        writeln!(stderr, "Verification Summary:").map_err(|e| StderrWriteError {
            details: "Failed to write summary header".to_string(),
            source: e,
        })?;

        writeln!(stderr, "  Total files: {}", self.total).map_err(|e| StderrWriteError {
            details: "Failed to write total count".to_string(),
            source: e,
        })?;

        writeln!(stderr, "  Passed: {} ✓", self.passed).map_err(|e| StderrWriteError {
            details: "Failed to write passed count".to_string(),
            source: e,
        })?;

        if self.failed > 0 {
            writeln!(stderr, "  Failed: {} ✗", self.failed).map_err(|e| StderrWriteError {
                details: "Failed to write failed count".to_string(),
                source: e,
            })?;
        }

        if self.missing > 0 {
            writeln!(stderr, "  Missing: {} ⚠", self.missing).map_err(|e| StderrWriteError {
                details: "Failed to write missing count".to_string(),
                source: e,
            })?;
        }

        if self.errors > 0 {
            writeln!(stderr, "  Errors: {} ⚠", self.errors).map_err(|e| StderrWriteError {
                details: "Failed to write errors count".to_string(),
                source: e,
            })?;
        }

        writeln!(stderr).map_err(|e| StderrWriteError {
            details: "Failed to write final blank line".to_string(),
            source: e,
        })?;

        Ok(())
    }

    /// Returns true if all verifications were successful.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.passed == self.total && self.failed == 0 && self.missing == 0 && self.errors == 0
    }
}

/// Checks if the terminal supports colors and styling
fn should_use_colors() -> bool {
    let no_color = std::env::var("NO_COLOR").is_ok();
    let term = std::env::var("TERM").unwrap_or_default();
    let is_dumb = term == "dumb";

    // Be aggressive about color support - if TERM suggests color capability
    // and colors aren't explicitly disabled, enable them.
    // Note: Even with this logic, prettytable's internal term crate might
    // still override our color choices based on its own TTY detection.
    let supports_color = !term.is_empty()
        && (term.contains("color")
            || term.contains("256")
            || term.starts_with("xterm")
            || term.starts_with("screen")
            || term == "alacritty"
            || term.contains("ghostty"));

    // Debug output to help diagnose color issues (commented out for production)
    // eprintln!("Color detection: NO_COLOR={}, TERM={}, supports_color={}, result={}",
    //     no_color, term, supports_color, !no_color && !is_dumb && supports_color);

    !no_color && !is_dumb && supports_color
}

/// Tiger Style: Helper function to create table with generic headers
fn create_table_with_generic_headers<T: PrettyTableRow>() -> Table {
    let mut table = Table::new();

    // Set table format with nice borders and styling
    table.set_format(*prettytable::format::consts::FORMAT_BOX_CHARS);

    // Add header row with styled cells using the trait method
    let headers = T::column_headers();
    let header_cells: Vec<Cell> = if should_use_colors() {
        headers
            .iter()
            .map(|&header| Cell::new(header).with_style(Attr::Bold))
            .collect()
    } else {
        headers.iter().map(|&header| Cell::new(header)).collect()
    };
    table.add_row(Row::new(header_cells));

    // Tiger Style: Postcondition assertion
    debug_assert!(table.len() == 1, "Table should have exactly one header row");

    table
}

/// Tiger Style: Helper function to add generic data rows to table
fn add_generic_data_rows_to_table<T: PrettyTableRow>(table: &mut Table, items: &[T]) {
    // Tiger Style: Precondition assertions
    debug_assert!(
        items.len() <= MAX_PAIRS_COUNT,
        "Items count exceeds maximum"
    );
    debug_assert!(
        !table.is_empty(),
        "Table must have header row before adding data"
    );

    for item in items {
        let row_data = item.format_row();
        let cells: Vec<Cell> = row_data.iter().map(|data| Cell::new(data)).collect();
        table.add_row(Row::new(cells));
    }

    // Tiger Style: Postcondition assertion
    debug_assert!(
        table.len() == items.len() + 1,
        "Table should have header plus data rows"
    );
}

// The old TableRowData struct and format_table_row_data function have been removed
// as they are now replaced by the PrettyTableRow trait implementation

/// Truncates a string to the specified length with ellipsis
fn truncate_string(s: &str, max_len: usize) -> String {
    // Precondition: Validate truncation parameters
    assert!(
        max_len > MIN_TRUNCATE_LENGTH,
        "Max length {max_len} must be greater than {MIN_TRUNCATE_LENGTH} for ellipsis"
    );
    assert!(
        max_len <= MAX_STRING_LENGTH,
        "Max length {max_len} exceeds maximum allowed {MAX_STRING_LENGTH}"
    );

    let result = if s.len() > max_len {
        format!("{}...", &s[..max_len - MIN_TRUNCATE_LENGTH])
    } else {
        s.to_string()
    };

    // Postcondition: Validate result
    assert!(
        result.len() <= max_len,
        "Result length {} must not exceed max length {}",
        result.len(),
        max_len
    );

    result
}

/// Formats hash for display with truncation if too long
fn format_hash_display(hash: &str) -> String {
    // Precondition: Input validation
    assert!(!hash.is_empty(), "Hash must not be empty");
    assert!(
        hash.len() <= MAX_STRING_LENGTH,
        "Hash length {} exceeds maximum {}",
        hash.len(),
        MAX_STRING_LENGTH
    );

    let result = if hash.len() > MAX_HASH_DISPLAY_LENGTH {
        format!(
            "{}...{}",
            &hash[..HASH_TRUNCATE_CHARS],
            &hash[hash.len() - HASH_TRUNCATE_CHARS..]
        )
    } else {
        hash.to_string()
    };

    // Postcondition: Validate result
    assert!(!result.is_empty(), "Hash display result must not be empty");

    result
}

/// Formats file path for display with truncation if too long
fn format_path_display(path: &Path) -> String {
    // Precondition: Input validation
    assert!(!path.as_os_str().is_empty(), "Path must not be empty");

    let path_str = path.to_string_lossy();
    let result = if path_str.len() > MAX_PATH_DISPLAY_LENGTH {
        format!("...{}", &path_str[path_str.len() - PATH_TRUNCATE_CHARS..])
    } else {
        path_str.to_string()
    };

    // Postcondition: Validate result
    assert!(!result.is_empty(), "Path display result must not be empty");

    result
}

/// Helper function to print generic table to stderr
fn print_generic_table_to_stderr<T: PrettyTableRow>(table: &Table) -> Result<(), CheckleError> {
    // Precondition: Validate table structure
    assert!(!table.is_empty(), "Table must not be empty");

    let mut stderr = io::stderr();

    writeln!(stderr, "\n{}:", T::table_title()).map_err(|e| StderrWriteError {
        details: "Failed to write table title".to_string(),
        source: e,
    })?;

    writeln!(stderr, "{table}").map_err(|e| StderrWriteError {
        details: "Failed to write table content".to_string(),
        source: e,
    })?;

    writeln!(stderr).map_err(|e| StderrWriteError {
        details: "Failed to write final newline".to_string(),
        source: e,
    })?;

    Ok(())
}

/// Converts a collection of enhanced pairs to basic `FileHashPair` for compatibility.
///
/// This utility function maintains backward compatibility with existing code
/// that expects the basic `FileHashPair` type.
///
/// # Arguments
/// * `enhanced_pairs` - Collection of enhanced file-hash pairs
///
/// # Returns
/// A vector of basic `FileHashPair` instances for compatibility.
#[must_use]
pub fn convert_to_basic_pairs(
    enhanced_pairs: Vec<FileHashPairWithMetadata>,
) -> Vec<crate::io::FileHashPair> {
    // Tiger Style: Precondition assertions
    debug_assert!(
        enhanced_pairs.len() <= MAX_PAIRS_COUNT,
        "Enhanced pairs count {} exceeds maximum {MAX_PAIRS_COUNT}",
        enhanced_pairs.len()
    );
    debug_assert!(
        enhanced_pairs.iter().all(|p| !p.hash().is_empty()),
        "All pairs must have non-empty hashes"
    );

    let result: Vec<_> = enhanced_pairs
        .into_iter()
        .map(FileHashPairWithMetadata::into_basic_pair)
        .collect();

    // Tiger Style: Postcondition assertions
    debug_assert!(
        result.iter().all(|p| !p.hash().is_empty()),
        "All converted pairs must have non-empty hashes"
    );
    debug_assert!(
        result.len() <= MAX_PAIRS_COUNT,
        "Result count exceeds maximum"
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1024), "1.00 KB");
        assert_eq!(format_file_size(1536), "1.50 KB");
        assert_eq!(format_file_size(1_048_576), "1.00 MB");
        assert_eq!(format_file_size(1_073_741_824), "1.00 GB");
        assert_eq!(format_file_size(1_099_511_627_776), "1.00 TB");
    }

    #[test]
    fn test_format_permissions() {
        assert_eq!(format_permissions(0o644), "rw-r--r--");
        assert_eq!(format_permissions(0o755), "rwxr-xr-x");
        assert_eq!(format_permissions(0o600), "rw-------");
        assert_eq!(format_permissions(0o000), "---------");
        assert_eq!(format_permissions(0o777), "rwxrwxrwx");
    }

    #[test]
    fn test_format_datetime() {
        // Test with known timestamp (2021-01-01 00:00:00 UTC)
        let result = format_datetime(1_609_459_200);
        // Should contain an absolute datetime in format "YYYY-MM-DD HH:MM:SS"
        assert!(result.len() > 3);
        assert!(!result.contains("Unknown"));
        // Check that it contains date components
        assert!(result.contains("2021"));
        assert!(result.contains(':'));
        assert!(result.contains('-'));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_file_hash_pair_with_metadata_creation() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file for test");
        fs::write(temp_file.path(), b"test content").expect("Failed to write test content");

        let metadata = fs::metadata(temp_file.path()).expect("Failed to get metadata");
        let hash = "abcdef1234567890".to_string();

        let pair =
            FileHashPairWithMetadata::new(temp_file.path().to_path_buf(), hash.clone(), &metadata)
                .expect("Should create pair with valid hash");

        assert_eq!(pair.hash(), &hash);
        assert_eq!(pair.file(), temp_file.path());
        assert!(pair.file_size() > 0);
        assert!(pair.modified_time().is_some());
        assert!(pair.permissions().is_some());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_file_hash_pair_with_fallback() {
        let path = std::path::PathBuf::from("nonexistent.txt");
        let hash = "abcdef1234567890".to_string();

        let pair = FileHashPairWithMetadata::new_with_fallback(path.clone(), hash.clone())
            .expect("Should create fallback pair with valid hash");

        assert_eq!(pair.hash(), &hash);
        assert_eq!(pair.file(), path.as_path());
        assert_eq!(pair.file_size(), 0);
        assert!(pair.modified_time().is_none());
        assert!(pair.permissions().is_none());
        assert_eq!(pair.file_extension(), Some("txt"));
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_convert_to_basic_pairs() {
        let temp_file1 = NamedTempFile::new().expect("Failed to create temp file 1");
        let temp_file2 = NamedTempFile::new().expect("Failed to create temp file 2");

        let enhanced = vec![
            FileHashPairWithMetadata::new_with_fallback(
                temp_file1.path().to_path_buf(),
                "abcdef1234567890".to_string(),
            )
            .expect("Should create fallback pair 1"),
            FileHashPairWithMetadata::new_with_fallback(
                temp_file2.path().to_path_buf(),
                "1234567890abcdef".to_string(),
            )
            .expect("Should create fallback pair 2"),
        ];

        let basic = convert_to_basic_pairs(enhanced);
        assert_eq!(basic.len(), 2);
        assert_eq!(basic[0].hash(), "abcdef1234567890");
        assert_eq!(basic[1].hash(), "1234567890abcdef");
    }

    #[test]
    fn test_display_pretty_table_empty() {
        let pairs = vec![];
        let result = display_pretty_table(&pairs);
        assert!(result.is_ok());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_display_pretty_table_with_data() {
        let pairs = vec![
            FileHashPairWithMetadata::new_with_fallback(
                std::path::PathBuf::from("test.txt"),
                "abcdef1234567890".to_string(),
            )
            .expect("Should create fallback pair"),
        ];

        let result = display_pretty_table(&pairs);
        assert!(result.is_ok());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_verification_result_creation() {
        let file = std::path::PathBuf::from("test.txt");
        let expected = "abcdef1234567890".to_string();
        let actual = "abcdef1234567890".to_string();
        let passed = true;

        let result =
            VerificationResult::new(file.clone(), expected.clone(), actual.clone(), passed)
                .expect("Should create verification result with valid hashes");

        assert_eq!(result.file(), file.as_path());
        assert_eq!(result.expected_hash(), &expected);
        assert_eq!(result.actual_hash(), &actual);
        assert!(result.passed());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_verification_result_failed() {
        let file = std::path::PathBuf::from("test.txt");
        let expected = "abcdef1234567890".to_string();
        let actual = "1234567890abcdef".to_string();
        let passed = false;

        let result =
            VerificationResult::new(file.clone(), expected.clone(), actual.clone(), passed)
                .expect("Should create verification result with valid hashes");

        assert_eq!(result.file(), file.as_path());
        assert_eq!(result.expected_hash(), &expected);
        assert_eq!(result.actual_hash(), &actual);
        assert!(!result.passed());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_verification_result_pretty_table_row() {
        let file = std::path::PathBuf::from("test.txt");
        let expected = "abcdef1234567890".to_string();
        let actual = "abcdef1234567890".to_string();
        let passed = true;

        let result = VerificationResult::new(file, expected, actual, passed)
            .expect("Should create verification result with valid hashes");

        let headers = VerificationResult::column_headers();
        assert_eq!(
            headers,
            vec![
                "File Path",
                "Status",
                "Expected Hash",
                "Computed Hash",
                "Size",
                "Modified",
                "Error Message"
            ]
        );

        let row = result.format_row();
        assert_eq!(row.len(), 7);
        assert!(row[0].contains("test.txt"));
        assert_eq!(row[1], "✓ PASS");
        assert!(row[2].contains("abcdef12"));
        assert!(row[3].contains("abcdef12"));
        assert_eq!(row[4], "-");

        assert_eq!(VerificationResult::table_title(), "Verification Results");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_verification_result_failed_formatting() {
        let file = std::path::PathBuf::from("test.txt");
        let expected = "abcdef1234567890".to_string();
        let actual = "1234567890abcdef".to_string();
        let passed = false;

        let result = VerificationResult::new(file, expected, actual, passed)
            .expect("Should create verification result with valid hashes");
        let row = result.format_row();

        assert_eq!(row.len(), 7);
        assert_eq!(row[1], "✗ FAIL");
    }

    #[test]
    fn test_verification_result_missing() {
        let file = std::path::PathBuf::from("missing.txt");
        let expected = "abcdef1234567890".to_string();

        let result = VerificationResult::new_missing(file.clone(), expected.clone());

        assert_eq!(result.file(), file.as_path());
        assert_eq!(result.expected_hash(), &expected);
        assert_eq!(result.actual_hash(), "");
        assert!(!result.passed());
        assert!(result.status().is_error());

        let row = result.format_row();
        assert_eq!(row.len(), 7);
        assert_eq!(row[1], "⚠ MISS");
        assert_eq!(row[3], "-"); // No computed hash
        assert_eq!(row[6], "File not found"); // Error message
    }

    #[test]
    fn test_verification_result_error() {
        let file = std::path::PathBuf::from("error.txt");
        let expected = "abcdef1234567890".to_string();
        let error_msg = "Permission denied".to_string();

        let result =
            VerificationResult::new_error(file.clone(), expected.clone(), error_msg.clone());

        assert_eq!(result.file(), file.as_path());
        assert_eq!(result.expected_hash(), &expected);
        assert_eq!(result.actual_hash(), "");
        assert!(!result.passed());
        assert!(result.status().is_error());
        assert_eq!(result.error_message(), Some(error_msg.as_str()));

        let row = result.format_row();
        assert_eq!(row.len(), 7);
        assert_eq!(row[1], "⚠ ERR");
        assert_eq!(row[3], "-"); // No computed hash
        assert_eq!(row[6], error_msg); // Error message
    }

    #[test]
    fn test_verification_status_methods() {
        use crate::prettyprint::VerificationStatus;

        let pass = VerificationStatus::Pass;
        assert!(pass.is_success());
        assert!(!pass.is_failure());
        assert!(!pass.is_error());
        assert_eq!(pass.display_string(), "PASS");
        assert_eq!(pass.symbol(), "✓");

        let fail = VerificationStatus::Fail;
        assert!(!fail.is_success());
        assert!(fail.is_failure());
        assert!(!fail.is_error());
        assert_eq!(fail.display_string(), "FAIL");
        assert_eq!(fail.symbol(), "✗");

        let missing = VerificationStatus::Missing;
        assert!(!missing.is_success());
        assert!(!missing.is_failure());
        assert!(missing.is_error());
        assert_eq!(missing.display_string(), "MISSING");
        assert_eq!(missing.symbol(), "⚠");

        let error = VerificationStatus::Error("Test error".to_string());
        assert!(!error.is_success());
        assert!(!error.is_failure());
        assert!(error.is_error());
        assert_eq!(error.display_string(), "ERROR: Test error");
        assert_eq!(error.symbol(), "⚠");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_verification_summary() {
        use crate::prettyprint::VerificationSummary;

        let results = vec![
            VerificationResult::new(
                std::path::PathBuf::from("pass1.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("fail1.txt"),
                "abcdef1234567890".to_string(),
                "1234567890abcdef".to_string(),
                false,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new_missing(
                std::path::PathBuf::from("missing1.txt"),
                "abcdef1234567890".to_string(),
            ),
            VerificationResult::new_error(
                std::path::PathBuf::from("error1.txt"),
                "abcdef1234567890".to_string(),
                "Permission denied".to_string(),
            ),
        ];

        let summary = VerificationSummary::from_results(&results);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.missing, 1);
        assert_eq!(summary.errors, 1);
        assert!(!summary.all_passed());

        // Test with all passed
        let all_passed_results = vec![
            VerificationResult::new(
                std::path::PathBuf::from("pass1.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("pass2.txt"),
                "1234567890abcdef".to_string(),
                "1234567890abcdef".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
        ];

        let all_passed_summary = VerificationSummary::from_results(&all_passed_results);
        assert!(all_passed_summary.all_passed());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_display_verification_table() {
        // Test that display_verification_table properly formats results
        let results = vec![
            VerificationResult::new(
                std::path::PathBuf::from("pass.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("fail.txt"),
                "abcdef1234567890".to_string(),
                "1234567890abcdef".to_string(),
                false,
            )
            .expect("Should create verification result with valid hashes"),
        ];

        // Should not panic and should handle multiple results
        let result = display_verification_table(&results);
        assert!(result.is_ok());
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_display_verification_table_with_summary() {
        // Test that summary correctly counts different statuses
        let results = vec![
            VerificationResult::new(
                std::path::PathBuf::from("pass1.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("pass2.txt"),
                "1234567890abcdef".to_string(),
                "1234567890abcdef".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("fail.txt"),
                "fedcba0987654321".to_string(),
                "1234567890abcdef".to_string(),
                false,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new_missing(
                std::path::PathBuf::from("missing.txt"),
                "deadbeef12345678".to_string(),
            ),
            VerificationResult::new_error(
                std::path::PathBuf::from("error.txt"),
                "cafebabe87654321".to_string(),
                "Permission denied".to_string(),
            ),
        ];

        // Should display table and summary without panicking
        let result = display_verification_table_with_summary(&results);
        assert!(result.is_ok());

        // Verify counts are correct by checking the internal state
        let passed_count = results.iter().filter(|r| r.passed()).count();
        let failed_count = results.iter().filter(|r| r.status().is_failure()).count();
        let missing_count = results
            .iter()
            .filter(|r| matches!(r.status(), VerificationStatus::Missing))
            .count();
        let error_count = results
            .iter()
            .filter(|r| matches!(r.status(), VerificationStatus::Error(_)))
            .count();

        assert_eq!(passed_count, 2);
        assert_eq!(failed_count, 1);
        assert_eq!(missing_count, 1);
        assert_eq!(error_count, 1);
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_colored_verification_rows_status_formatting() {
        // Test that status column gets proper formatting with symbols and text
        let results = vec![
            VerificationResult::new(
                std::path::PathBuf::from("test.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
        ];

        let mut table = create_table_with_generic_headers::<VerificationResult>();
        add_colored_verification_rows(&mut table, &results);

        // Table should have header + data rows
        assert_eq!(table.len(), 2);

        // Verify the formatted row contains proper status display
        let row = results[0].format_row();
        assert_eq!(row[1], "✓ PASS"); // Status column should have symbol and text
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_file_hash_pair_pretty_table_row() {
        let pair = FileHashPairWithMetadata::new_with_fallback(
            std::path::PathBuf::from("test.txt"),
            "abcdef1234567890".to_string(),
        )
        .expect("Should create fallback pair with valid hash");

        let headers = FileHashPairWithMetadata::column_headers();
        assert_eq!(
            headers,
            vec![
                "Hash",
                "File Path",
                "Size",
                "Modified",
                "Extension",
                "Permissions"
            ]
        );

        let row = pair.format_row();
        assert_eq!(row.len(), 6);
        assert!(row[0].contains("abcdef12"));
        assert!(row[1].contains("test.txt"));
        assert_eq!(row[2], "0 B");
        assert_eq!(row[3], "Unknown");
        assert_eq!(row[4], "txt");
        assert_eq!(row[5], "Unknown");

        assert_eq!(FileHashPairWithMetadata::table_title(), "Hash Results");
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn test_generic_display_table() {
        let verifications = vec![
            VerificationResult::new(
                std::path::PathBuf::from("test1.txt"),
                "abcdef1234567890".to_string(),
                "abcdef1234567890".to_string(),
                true,
            )
            .expect("Should create verification result with valid hashes"),
            VerificationResult::new(
                std::path::PathBuf::from("test2.txt"),
                "1234567890abcdef".to_string(),
                "fedcba0987654321".to_string(),
                false,
            )
            .expect("Should create verification result with valid hashes"),
        ];

        let result = display_table(&verifications);
        assert!(result.is_ok());
    }

    #[test]
    fn test_generic_display_table_empty() {
        let verifications: Vec<VerificationResult> = vec![];
        let result = display_table(&verifications);
        assert!(result.is_ok());
    }
}
