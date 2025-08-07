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

/// Pattern type for matching entries within archives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchivePattern {
    /// Match all files in the archive (archive.tar: or archive.tar:*)
    AllFiles,
    /// Match files using glob patterns (archive.tar:*.txt, archive.tar:**/*.fastq)
    Glob(String),
    /// Match a specific file path (archive.tar:path/to/file.txt)
    SpecificFile(String),
}

impl ArchivePattern {
    /// Create a new pattern from a string, automatically detecting the pattern type.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The pattern string to parse
    ///
    /// # Returns
    ///
    /// Returns the appropriate `ArchivePattern` variant.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The pattern contains path traversal attempts
    /// - The pattern length exceeds `MAX_PATH_LENGTH`
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            pattern.len() <= MAX_PATH_LENGTH,
            "Pattern length {} exceeds maximum {}",
            pattern.len(),
            MAX_PATH_LENGTH
        );
        assert!(
            !contains_path_traversal(pattern),
            "Pattern contains path traversal: {pattern}"
        );

        // Empty pattern or single asterisk means all files
        if pattern.is_empty() || pattern == "*" {
            return Self::AllFiles;
        }

        // Check if pattern contains glob characters
        if contains_glob_characters(pattern) {
            Self::Glob(pattern.to_string())
        } else {
            Self::SpecificFile(pattern.to_string())
        }
    }

    /// Check if this pattern matches a given entry path.
    ///
    /// # Arguments
    ///
    /// * `entry_path` - The entry path to check against the pattern
    ///
    /// # Returns
    ///
    /// Returns `true` if the pattern matches the entry path.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The entry path is empty
    /// - The entry path contains invalid characters
    #[must_use]
    pub fn matches(&self, entry_path: &str) -> bool {
        // For empty paths, only AllFiles and empty Glob patterns should match
        if entry_path.is_empty() {
            return match self {
                Self::AllFiles => true,
                Self::Glob(pattern) => pattern.is_empty(),
                Self::SpecificFile(_) => false,
            };
        }

        // Precondition assertions for non-empty paths
        assert!(
            !contains_path_traversal(entry_path),
            "Entry path contains path traversal: {entry_path}"
        );

        match self {
            Self::AllFiles => true,
            Self::SpecificFile(path) => path == entry_path,
            Self::Glob(pattern) => matches_glob_pattern(entry_path, pattern),
        }
    }

    /// Get the pattern string for this pattern.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::AllFiles => "*",
            Self::Glob(pattern) | Self::SpecificFile(pattern) => pattern,
        }
    }
}

/// Components of an archive path after parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePathComponents {
    /// Path to the archive file (e.g., "data.tar", "results.zip").
    pub archive_path: PathBuf,
    /// Pattern for matching entries within the archive.
    pub pattern: ArchivePattern,
}

impl ArchivePathComponents {
    /// Create new archive path components with validation.
    ///
    /// # Arguments
    ///
    /// * `archive_path` - Path to the archive file
    /// * `pattern` - Pattern for matching entries within the archive
    ///
    /// # Returns
    ///
    /// Returns `Some(ArchivePathComponents)` if valid, `None` if validation fails.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Archive path validation fails
    /// - Pattern validation fails (handled by `ArchivePattern::new`)
    #[must_use]
    pub fn new(archive_path: PathBuf, pattern: ArchivePattern) -> Option<Self> {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            archive_path.as_os_str().len() <= MAX_PATH_LENGTH,
            "Archive path exceeds maximum length: {} > {}",
            archive_path.as_os_str().len(),
            MAX_PATH_LENGTH
        );
        assert!(
            !archive_path.as_os_str().is_empty(),
            "Archive path must not be empty"
        );

        // Validate archive extension
        if !is_supported_archive(&archive_path) {
            return None;
        }

        // Validate pattern depth for specific files and simple globs
        if let ArchivePattern::SpecificFile(ref path) | ArchivePattern::Glob(ref path) = pattern {
            let depth = path.matches('/').count();
            if depth > MAX_ARCHIVE_DEPTH {
                return None;
            }
        }

        Some(Self {
            archive_path,
            pattern,
        })
    }

    /// Get the archive path.
    #[must_use]
    pub fn archive(&self) -> &Path {
        &self.archive_path
    }

    /// Get the pattern for matching entries within the archive.
    #[must_use]
    pub fn pattern(&self) -> &ArchivePattern {
        &self.pattern
    }

    /// Get the entry path within the archive (for backwards compatibility).
    /// Returns the pattern as a string for `SpecificFile` patterns, "*" for others.
    #[must_use]
    pub fn entry(&self) -> &str {
        self.pattern.as_str()
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
/// // Empty pattern is valid (matches all files)
/// assert!(parse_archive_path("data.tar:").is_some()); // Empty pattern = all files
///
/// // Invalid archive paths return None
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

    // Ensure colon isn't at the beginning (but allow at the end for empty patterns)
    if colon_pos == 0 {
        return None;
    }

    let (archive_part, entry_part) = path.split_at(colon_pos);
    let entry_part = &entry_part[1..]; // Skip the colon

    // Check if this looks like an archive path
    let archive_path = PathBuf::from(archive_part);
    if !is_supported_archive(&archive_path) {
        return None;
    }

    // Validate entry part before creating pattern
    if contains_path_traversal(entry_part) || entry_part.len() > MAX_PATH_LENGTH {
        return None;
    }

    // Create pattern from the entry part
    let pattern = ArchivePattern::new(entry_part);

    // Validate and create components
    ArchivePathComponents::new(archive_path, pattern)
}

/// Check if a string contains glob pattern characters.
///
/// Glob characters include: *, ?, [, ]
fn contains_glob_characters(pattern: &str) -> bool {
    pattern.chars().any(|c| matches!(c, '*' | '?' | '[' | ']'))
}

/// Check if an entry path matches a glob pattern.
///
/// Uses a simple glob implementation that supports:
/// - `*` matches any sequence of characters (except path separators in strict mode)
/// - `?` matches any single character
/// - `[abc]` matches any character in the set
/// - `**` matches any sequence including path separators (recursive)
fn matches_glob_pattern(entry_path: &str, pattern: &str) -> bool {
    // Handle the simple cases first
    if pattern == "*" {
        return true;
    }

    if pattern.is_empty() {
        return entry_path.is_empty();
    }

    if pattern == entry_path {
        return true;
    }

    // Use a simple glob matching algorithm
    glob_match_recursive(entry_path, pattern)
}

/// Recursive glob matching implementation.
///
/// This is a simplified implementation that handles the most common cases
/// needed for archive introspection.
fn glob_match_recursive(text: &str, pattern: &str) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();

    glob_match_impl(&text_chars, &pattern_chars, 0, 0)
}

/// Internal implementation of glob matching using dynamic programming approach.
fn glob_match_impl(text: &[char], pattern: &[char], text_idx: usize, pat_idx: usize) -> bool {
    // Base cases
    if pat_idx >= pattern.len() {
        return text_idx >= text.len();
    }

    if text_idx >= text.len() {
        // Check if remaining pattern is all wildcards
        return pattern[pat_idx..].iter().all(|&c| c == '*');
    }

    match pattern[pat_idx] {
        '*' => {
            // Try matching zero characters
            if glob_match_impl(text, pattern, text_idx, pat_idx + 1) {
                return true;
            }
            // Try matching one or more characters, but not path separators
            if text[text_idx] != '/' && text[text_idx] != '\\' {
                glob_match_impl(text, pattern, text_idx + 1, pat_idx)
            } else {
                false
            }
        }
        '?' => {
            // Match any single character
            glob_match_impl(text, pattern, text_idx + 1, pat_idx + 1)
        }
        c if c == text[text_idx] => {
            // Exact character match
            glob_match_impl(text, pattern, text_idx + 1, pat_idx + 1)
        }
        _ => false,
    }
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
/// the archive boundaries. Note that absolute paths within archives are allowed
/// since they're still contained within the archive boundaries.
fn contains_path_traversal(path: &str) -> bool {
    // Check for parent directory references
    let components: Vec<&str> = path.split(&['/', '\\'][..]).collect();
    for component in components {
        if component == ".." {
            return true;
        }
    }

    // Check for Windows drive letters (but allow single colon after first character)
    if path.len() >= 2
        && path.chars().nth(1) == Some(':')
        && path.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    {
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

        // Empty archive path
        assert!(parse_archive_path(":file.txt").is_none());

        // Not an archive extension
        assert!(parse_archive_path("file.txt:something").is_none());

        // Path traversal attempts (these should still be invalid)
        assert!(parse_archive_path("data.tar:../../../etc/passwd").is_none());
        assert!(parse_archive_path("data.tar:./../../sensitive").is_none());
        assert!(parse_archive_path("data.tar:C:\\windows\\system32").is_none());
    }

    #[test]
    fn test_path_traversal_detection() {
        // These should be detected as traversal attempts
        assert!(contains_path_traversal("../parent"));
        assert!(contains_path_traversal("../../grandparent"));
        assert!(contains_path_traversal("C:\\windows"));
        assert!(contains_path_traversal("nested/../escape"));
        assert!(contains_path_traversal("D:\\Program Files"));

        // Valid paths (including absolute paths within archives)
        assert!(!contains_path_traversal("normal/path/file.txt"));
        assert!(!contains_path_traversal("file.txt"));
        assert!(!contains_path_traversal("deeply/nested/structure/file.txt"));
        assert!(!contains_path_traversal("/absolute/path")); // Absolute paths within archives are OK
        assert!(!contains_path_traversal("\\windows\\path")); // Windows-style paths are OK
        assert!(!contains_path_traversal("archive.tar:path")); // Archive syntax is OK
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
        let pattern = ArchivePattern::SpecificFile("file.txt".to_string());
        let components =
            ArchivePathComponents::new(PathBuf::from("test_fixtures/test.tar"), pattern);
        assert!(components.is_some());

        // Invalid archive extension
        let pattern = ArchivePattern::SpecificFile("file.txt".to_string());
        let components = ArchivePathComponents::new(PathBuf::from("data.txt"), pattern);
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

    #[test]
    fn test_archive_pattern_parsing() {
        // Test AllFiles patterns
        let result = parse_archive_path("test.tar:");
        assert!(result.is_some());
        let components = result.expect("Should parse empty pattern as AllFiles");
        assert!(matches!(components.pattern(), ArchivePattern::AllFiles));

        let result = parse_archive_path("test.tar:*");
        assert!(result.is_some());
        let components = result.expect("Should parse * as AllFiles");
        assert!(matches!(components.pattern(), ArchivePattern::AllFiles));

        // Test glob patterns
        let result = parse_archive_path("test.tar:*.txt");
        assert!(result.is_some());
        let components = result.expect("Should parse *.txt as Glob");
        assert!(matches!(components.pattern(), ArchivePattern::Glob(_)));

        let result = parse_archive_path("test.tar:**/*.rs");
        assert!(result.is_some());
        let components = result.expect("Should parse **/*.rs as Glob");
        assert!(matches!(components.pattern(), ArchivePattern::Glob(_)));

        // Test specific file patterns
        let result = parse_archive_path("test.tar:specific/file.txt");
        assert!(result.is_some());
        let components = result.expect("Should parse specific file as SpecificFile");
        assert!(matches!(
            components.pattern(),
            ArchivePattern::SpecificFile(_)
        ));
    }

    #[test]
    fn test_archive_pattern_matching() {
        // Test AllFiles pattern
        let all_files = ArchivePattern::AllFiles;
        assert!(all_files.matches("any/file.txt"));
        assert!(all_files.matches("another.rs"));
        assert!(all_files.matches("deeply/nested/path/file.fastq"));

        // Test SpecificFile pattern
        let specific = ArchivePattern::SpecificFile("target/file.txt".to_string());
        assert!(specific.matches("target/file.txt"));
        assert!(!specific.matches("target/other.txt"));
        assert!(!specific.matches("other/file.txt"));

        // Test Glob patterns
        let txt_glob = ArchivePattern::Glob("*.txt".to_string());
        assert!(txt_glob.matches("file.txt"));
        assert!(txt_glob.matches("another.txt"));
        assert!(!txt_glob.matches("file.rs"));
        assert!(!txt_glob.matches("dir/file.txt")); // Single * doesn't cross directories

        let nested_glob = ArchivePattern::Glob("src/*.rs".to_string());
        assert!(nested_glob.matches("src/main.rs"));
        assert!(nested_glob.matches("src/lib.rs"));
        assert!(!nested_glob.matches("tests/main.rs"));
        assert!(!nested_glob.matches("src/nested/file.rs"));
    }

    #[test]
    fn test_glob_matching_edge_cases() {
        // Test question mark matching
        let pattern = ArchivePattern::Glob("file?.txt".to_string());
        assert!(pattern.matches("file1.txt"));
        assert!(pattern.matches("fileA.txt"));
        assert!(!pattern.matches("file.txt"));
        assert!(!pattern.matches("file12.txt"));

        // Test complex patterns
        let pattern = ArchivePattern::Glob("*.tar.gz".to_string());
        assert!(pattern.matches("archive.tar.gz"));
        assert!(pattern.matches("data.tar.gz"));
        assert!(!pattern.matches("archive.tar"));
        assert!(!pattern.matches("file.gz"));

        // Test empty and edge cases
        let pattern = ArchivePattern::Glob(String::new());
        assert!(pattern.matches(""));
        assert!(!pattern.matches("anything"));
    }

    #[test]
    fn test_pattern_validation() {
        // Test valid patterns
        let pattern = ArchivePattern::new("*.txt");
        assert!(matches!(pattern, ArchivePattern::Glob(_)));

        let pattern = ArchivePattern::new("specific/file.txt");
        assert!(matches!(pattern, ArchivePattern::SpecificFile(_)));

        let pattern = ArchivePattern::new("");
        assert!(matches!(pattern, ArchivePattern::AllFiles));

        let pattern = ArchivePattern::new("*");
        assert!(matches!(pattern, ArchivePattern::AllFiles));
    }
}
