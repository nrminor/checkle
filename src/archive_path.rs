//! Archive path parsing and validation.
//!
//! This module provides functionality to parse and validate paths that reference
//! files within archives using the syntax `archive.tar:internal/file/path.txt`.
//!
//! # Examples
//!
//! ```
//! use checkle::archive_path::{parse_archive_path, ArchivePathComponents};
//!
//! // Parse a TAR archive path
//! let components = parse_archive_path("data.tar:sequences/sample.fastq");
//! assert!(components.is_some());
//!
//! // Parse a ZIP archive path  
//! let components = parse_archive_path("results.zip:output/final.bam");
//! assert!(components.is_some());
//!
//! // Regular file paths return None
//! let components = parse_archive_path("/regular/file.txt");
//! assert!(components.is_none());
//! ```

use std::path::{Path, PathBuf};

use crate::constants::{MAX_ARCHIVE_DEPTH, MAX_PATH_LENGTH};

/// Components of an archive path after parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePathComponents {
    /// Path to the archive file (e.g., "data.tar", "results.zip").
    pub archive_path: PathBuf,
    /// Path within the archive (e.g., "sequences/sample.fastq").
    pub entry_path: String,
}

impl ArchivePathComponents {
    /// Create new archive path components with validation.
    ///
    /// # Arguments
    ///
    /// * `archive_path` - Path to the archive file
    /// * `entry_path` - Path within the archive
    ///
    /// # Returns
    ///
    /// Returns `Some(ArchivePathComponents)` if valid, `None` if validation fails.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The entry path is empty
    /// - The entry path exceeds `MAX_PATH_LENGTH`
    #[must_use]
    pub fn new(archive_path: PathBuf, entry_path: String) -> Option<Self> {
        // Precondition assertions
        assert!(!entry_path.is_empty(), "Entry path must not be empty");
        assert!(
            entry_path.len() <= MAX_PATH_LENGTH,
            "Entry path exceeds maximum length: {} > {}",
            entry_path.len(),
            MAX_PATH_LENGTH
        );

        // Validate archive extension
        if !is_supported_archive(&archive_path) {
            return None;
        }

        // Validate entry path doesn't contain path traversal attempts
        if contains_path_traversal(&entry_path) {
            return None;
        }

        // Check maximum nesting depth
        let depth = entry_path.matches('/').count();
        if depth > MAX_ARCHIVE_DEPTH {
            return None;
        }

        Some(Self {
            archive_path,
            entry_path,
        })
    }

    /// Get the archive path.
    #[must_use]
    pub fn archive(&self) -> &Path {
        &self.archive_path
    }

    /// Get the entry path within the archive.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry_path
    }
}

/// Parse an archive path string into its components.
///
/// Archive paths use the syntax `archive.tar:internal/file/path.txt` where:
/// - The archive path comes before the colon
/// - The internal file path comes after the colon
/// - Supported archive extensions: .tar, .tar.gz, .tar.bz2, .tar.xz, .tgz, .zip
///
/// # Arguments
///
/// * `path` - The path string to parse
///
/// # Returns
///
/// Returns `Some(ArchivePathComponents)` if the path is a valid archive path,
/// or `None` if it's a regular file path or invalid.
///
/// # Panics
///
/// Panics if:
/// - The path is empty
/// - The path exceeds twice `MAX_PATH_LENGTH`
///
/// # Examples
///
/// ```
/// use checkle::archive_path::parse_archive_path;
///
/// // Valid archive paths
/// assert!(parse_archive_path("data.tar:file.txt").is_some());
/// assert!(parse_archive_path("archive.tar.gz:nested/file.txt").is_some());
/// assert!(parse_archive_path("results.zip:output/data.csv").is_some());
///
/// // Regular file paths return None
/// assert!(parse_archive_path("/regular/file.txt").is_none());
/// assert!(parse_archive_path("file.txt").is_none());
///
/// // Invalid archive paths return None
/// assert!(parse_archive_path("data.tar:").is_none()); // Empty entry
/// assert!(parse_archive_path("data.tar:../../../etc/passwd").is_none()); // Path traversal
/// ```
#[must_use]
pub fn parse_archive_path(path: &str) -> Option<ArchivePathComponents> {
    // Precondition assertions
    assert!(!path.is_empty(), "Path must not be empty");
    assert!(
        path.len() <= MAX_PATH_LENGTH * 2,
        "Path exceeds maximum length"
    );

    // Find the last colon that could be part of archive syntax
    let colon_pos = path.rfind(':')?;

    // Ensure colon isn't at the beginning or end
    if colon_pos == 0 || colon_pos == path.len() - 1 {
        return None;
    }

    let (archive_part, entry_part) = path.split_at(colon_pos);
    let entry_part = &entry_part[1..]; // Skip the colon

    // Check if this looks like an archive path
    let archive_path = PathBuf::from(archive_part);
    if !is_supported_archive(&archive_path) {
        return None;
    }

    // Validate and create components
    ArchivePathComponents::new(archive_path, entry_part.to_string())
}

/// Check if a path has a supported archive extension.
///
/// Supported extensions:
/// - TAR: .tar, .tar.gz, .tar.bz2, .tar.xz, .tar.zst, .tgz, .tbz2, .txz
/// - ZIP: .zip
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_supported_archive(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();

    // Check TAR variants (already case-insensitive due to to_lowercase())
    if path_str.ends_with(".tar")
        || path_str.ends_with(".tar.gz")
        || path_str.ends_with(".tar.bz2")
        || path_str.ends_with(".tar.xz")
        || path_str.ends_with(".tar.zst")
        || path_str.ends_with(".tgz")
        || path_str.ends_with(".tbz2")
        || path_str.ends_with(".txz")
    {
        return true;
    }

    // Check ZIP
    if path_str.ends_with(".zip") {
        return true;
    }

    false
}

/// Check if a path contains directory traversal attempts.
///
/// This prevents malicious paths like "../../../etc/passwd" from escaping
/// the archive boundaries.
fn contains_path_traversal(path: &str) -> bool {
    // Check for absolute paths
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }

    // Check for parent directory references
    let components: Vec<&str> = path.split(&['/', '\\'][..]).collect();
    for component in components {
        if component == ".." || component == "." {
            return true;
        }
    }

    // Check for Windows drive letters
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::assertions_on_constants
    )]
    use super::*;

    #[test]
    fn test_parse_valid_archive_paths() {
        // TAR archives
        let result = parse_archive_path("data.tar:file.txt");
        assert!(result.is_some());
        let components = result.expect("Should parse valid TAR path");
        assert_eq!(components.archive(), Path::new("data.tar"));
        assert_eq!(components.entry(), "file.txt");

        // Compressed TAR
        let result = parse_archive_path("archive.tar.gz:nested/path/file.fastq");
        assert!(result.is_some());
        let components = result.expect("Should parse valid compressed TAR path");
        assert_eq!(components.archive(), Path::new("archive.tar.gz"));
        assert_eq!(components.entry(), "nested/path/file.fastq");

        // ZIP archives
        let result = parse_archive_path("results.zip:output/data.csv");
        assert!(result.is_some());
        let components = result.expect("Should parse valid ZIP path");
        assert_eq!(components.archive(), Path::new("results.zip"));
        assert_eq!(components.entry(), "output/data.csv");
    }

    #[test]
    fn test_parse_invalid_archive_paths() {
        // No colon
        assert!(parse_archive_path("regular_file.txt").is_none());

        // Empty entry path
        assert!(parse_archive_path("data.tar:").is_none());

        // Empty archive path
        assert!(parse_archive_path(":file.txt").is_none());

        // Not an archive extension
        assert!(parse_archive_path("file.txt:something").is_none());

        // Path traversal attempts
        assert!(parse_archive_path("data.tar:../../../etc/passwd").is_none());
        assert!(parse_archive_path("data.tar:./../../sensitive").is_none());
        assert!(parse_archive_path("data.tar:/absolute/path").is_none());
    }

    #[test]
    fn test_path_traversal_detection() {
        assert!(contains_path_traversal("../parent"));
        assert!(contains_path_traversal("../../grandparent"));
        assert!(contains_path_traversal("./current"));
        assert!(contains_path_traversal("/absolute/path"));
        assert!(contains_path_traversal("\\windows\\path"));
        assert!(contains_path_traversal("C:\\windows"));
        assert!(contains_path_traversal("nested/../escape"));

        // Valid paths
        assert!(!contains_path_traversal("normal/path/file.txt"));
        assert!(!contains_path_traversal("file.txt"));
        assert!(!contains_path_traversal("deeply/nested/structure/file.txt"));
    }

    #[test]
    fn test_supported_archive_extensions() {
        // TAR variants
        assert!(is_supported_archive(Path::new("file.tar")));
        assert!(is_supported_archive(Path::new("file.tar.gz")));
        assert!(is_supported_archive(Path::new("file.tar.bz2")));
        assert!(is_supported_archive(Path::new("file.tar.xz")));
        assert!(is_supported_archive(Path::new("file.tgz")));
        assert!(is_supported_archive(Path::new("FILE.TAR.GZ"))); // Case insensitive

        // ZIP
        assert!(is_supported_archive(Path::new("archive.zip")));
        assert!(is_supported_archive(Path::new("ARCHIVE.ZIP")));

        // Non-archives
        assert!(!is_supported_archive(Path::new("file.txt")));
        assert!(!is_supported_archive(Path::new("file.rs")));
        assert!(!is_supported_archive(Path::new("file")));
    }

    #[test]
    fn test_archive_path_components_validation() {
        // Valid components
        let components =
            ArchivePathComponents::new(PathBuf::from("data.tar"), "file.txt".to_string());
        assert!(components.is_some());

        // Invalid archive extension
        let components =
            ArchivePathComponents::new(PathBuf::from("data.txt"), "file.txt".to_string());
        assert!(components.is_none());

        // Path traversal in entry
        let components =
            ArchivePathComponents::new(PathBuf::from("data.tar"), "../escape.txt".to_string());
        assert!(components.is_none());
    }

    #[test]
    fn test_complex_archive_paths() {
        // Path with multiple colons (Windows drive in archive name)
        let result = parse_archive_path("C:\\data\\archive.tar:internal/file.txt");
        assert!(result.is_some());
        let components = result.expect("Should parse path with colons");
        assert_eq!(components.entry(), "internal/file.txt");

        // Unicode in paths
        let result = parse_archive_path("données.tar:fichier/données.txt");
        assert!(result.is_some());
        let components = result.expect("Should parse Unicode path");
        assert_eq!(components.entry(), "fichier/données.txt");
    }
}
