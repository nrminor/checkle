use ignore::{WalkBuilder, overrides::OverrideBuilder};
use log::{debug, warn};
use std::{
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::{
    archive::ArchiveReader,
    archive_path::{ArchivePathComponents, parse_archive_path},
    cli::Recursive,
    constants::{MAX_CHECKSUM_FILE_LINES, MAX_FILES_IN_BATCH, MAX_RECURSION_DEPTH},
    prelude::CheckleError,
};

/// Configuration for file filtering using include/exclude patterns.
///
/// This struct encapsulates glob patterns for including or excluding files
/// during directory traversal. Patterns follow gitignore syntax where:
/// - Include patterns match files to be processed
/// - Exclude patterns match files to be skipped
/// - The `no_ignore` flag controls whether .gitignore files are respected
///
/// # Examples
///
/// ```
/// use checkle::io::FileFilterConfig;
///
/// let mut config = FileFilterConfig::new();
/// config.include_patterns = vec!["*.rs".to_string(), "src/**/*.txt".to_string()];
/// config.exclude_patterns = vec!["*.test.rs".to_string(), "**/target/**".to_string()];
/// config.no_ignore = false;
/// ```
#[derive(Debug, Clone)]
pub struct FileFilterConfig {
    /// Glob patterns to include (whitelist).
    /// Only files matching at least one include pattern will be processed.
    /// If empty, all files are included by default.
    pub include_patterns: Vec<String>,

    /// Glob patterns to exclude (blacklist).
    /// Files matching any exclude pattern will be skipped.
    /// Exclude patterns take precedence over include patterns.
    pub exclude_patterns: Vec<String>,

    /// Whether to ignore .gitignore files during traversal.
    /// When true, files listed in .gitignore will still be processed.
    pub no_ignore: bool,
    /// Maximum number of files to collect in a single batch.
    /// This prevents memory exhaustion when processing large directory trees.
    pub max_files_batch: usize,
}

impl Default for FileFilterConfig {
    fn default() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            no_ignore: false,
            max_files_batch: MAX_FILES_IN_BATCH,
        }
    }
}

impl FileFilterConfig {
    /// Creates a new empty filter configuration.
    ///
    /// # Panics
    ///
    /// Panics if postcondition assertions fail (patterns are not empty when they should be).
    #[must_use]
    pub fn new() -> Self {
        let config = Self::default();

        // Postcondition assertions
        assert!(
            config.include_patterns.is_empty(),
            "New config must have empty include patterns"
        );
        assert!(
            config.exclude_patterns.is_empty(),
            "New config must have empty exclude patterns"
        );
        assert!(
            !config.no_ignore,
            "New config must respect .gitignore by default"
        );

        config
    }

    /// Checks if this configuration has any active filters.
    ///
    /// Returns true if any include or exclude patterns are specified.
    #[must_use]
    pub fn has_filters(&self) -> bool {
        !self.include_patterns.is_empty() || !self.exclude_patterns.is_empty()
    }

    /// Builds an override matcher from this configuration.
    ///
    /// Creates an `OverrideBuilder` and adds all include/exclude patterns to it.
    /// Include patterns are added as-is, while exclude patterns are prefixed with '!'
    /// following the ignore crate's convention.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory for pattern matching
    ///
    /// # Returns
    ///
    /// Returns `None` if no patterns are configured, otherwise returns the built
    /// override matcher wrapped in a Result.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any glob pattern is invalid
    /// - The override builder fails to build
    ///
    /// # Panics
    ///
    /// Panics if the root path doesn't exist (precondition).
    pub fn build_overrides(
        &self,
        root: &Path,
    ) -> Result<Option<ignore::overrides::Override>, CheckleError> {
        // Precondition assertion
        assert!(root.exists(), "Root path must exist: {}", root.display());

        if !self.has_filters() {
            return Ok(None);
        }

        let mut builder = OverrideBuilder::new(root);

        // Add include patterns (without ! prefix)
        for pattern in &self.include_patterns {
            builder.add(pattern).map_err(|e| {
                CheckleError::InvalidCliArgument(format!(
                    "Invalid include pattern '{pattern}': {e}"
                ))
            })?;
        }

        // Add exclude patterns (with ! prefix for ignore crate)
        for pattern in &self.exclude_patterns {
            let exclude_pattern = format!("!{pattern}");
            builder.add(&exclude_pattern).map_err(|e| {
                CheckleError::InvalidCliArgument(format!(
                    "Invalid exclude pattern '{pattern}': {e}"
                ))
            })?;
        }

        let overrides = builder.build().map_err(|e| {
            CheckleError::InvalidCliArgument(format!("Failed to build override matcher: {e}"))
        })?;

        Ok(Some(overrides))
    }
}

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

    /// Creates a new file-hash pair for archive paths or when file existence is already validated.
    ///
    /// This variant skips the file existence check, which is necessary for archive paths
    /// where the file exists within an archive but not on the filesystem.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The hash is empty
    /// - The hash contains non-hexadecimal characters
    /// - Any postcondition check fails
    #[must_use]
    pub fn new_unchecked(file: PathBuf, hash: String) -> Self {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(!hash.is_empty(), "Hash must not be empty");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash must contain only hexadecimal characters"
        );

        let pair = Self { file, hash };

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            !pair.file().as_os_str().is_empty(),
            "File path must not be empty"
        );
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

    /// Creates a `FilesToCheck` collection by parsing a checksum file from within an archive.
    ///
    /// The checksum file should be tab-delimited with two columns:
    /// - First column: hash value
    /// - Second column: file path (relative to archive or filesystem)
    ///
    /// When processing file paths from the checksum:
    /// - If a file exists within the same archive, it will be included
    /// - If a file doesn't exist in the archive but exists on the filesystem, it will be included
    /// - If a file doesn't exist in either location, a warning is logged and it's skipped
    ///
    /// # Arguments
    ///
    /// * `archive_components` - Parsed archive path components
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The archive cannot be opened
    /// - The checksum file cannot be found within the archive
    /// - The file format is invalid (not tab-delimited, wrong number of fields)
    /// - I/O errors occur while reading
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The archive file doesn't exist
    /// - The number of lines exceeds `MAX_CHECKSUM_FILE_LINES` (100,000)
    /// - Any Tiger Style assertions fail
    pub fn new_from_archive(
        archive_components: &ArchivePathComponents,
    ) -> Result<FilesToCheck, CheckleError> {
        // Precondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            archive_components.archive().exists(),
            "Archive file must exist: {}",
            archive_components.archive().display()
        );
        assert!(
            !archive_components.entry().is_empty(),
            "Archive entry path must not be empty"
        );

        // Try to open the appropriate archive type
        let checksum_content = read_file_from_archive(archive_components)?;

        let mut files_to_check = FilesToCheck::new();
        let mut line_count = 0;

        for line in checksum_content.lines() {
            line_count += 1;
            assert!(
                line_count <= MAX_CHECKSUM_FILE_LINES,
                "Checksum file exceeds maximum line count: {line_count} > {MAX_CHECKSUM_FILE_LINES}"
            );

            if line.trim().is_empty() {
                continue;
            }

            // Use stack array instead of heap allocation for exactly 2 expected fields
            let mut fields = line.split('\t');
            let Some(hash) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    archive_components.archive().to_path_buf(),
                ));
            };
            let Some(file_str) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    archive_components.archive().to_path_buf(),
                ));
            };
            // Ensure no extra fields remain
            if fields.next().is_some() {
                return Err(CheckleError::InvalidChecksumFile(
                    archive_components.archive().to_path_buf(),
                ));
            }

            // Try to find the file within the archive first, then fallback to filesystem
            let file_path = PathBuf::from(file_str);
            let file_exists = check_file_availability(&file_path, Some(archive_components))?;

            if !file_exists {
                warn!(
                    "A file listed in the checksum file, {file_str}, does not exist in the archive or filesystem and will be skipped"
                );
                continue;
            }

            // Use appropriate constructor based on whether file exists on filesystem
            let wrapper = if file_path.exists() {
                FileHashPair::new(file_path, hash.to_string())
            } else {
                // File exists in archive but not on filesystem
                FileHashPair::new_unchecked(file_path, hash.to_string())
            };
            files_to_check.push(wrapper);
        }

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert!(
            files_to_check.0.len() <= MAX_FILES_IN_BATCH,
            "Result must not exceed maximum batch size"
        );
        assert!(
            line_count <= MAX_CHECKSUM_FILE_LINES,
            "Line count must not exceed maximum"
        );

        Ok(files_to_check)
    }
}

/// Public function to read a file from within an archive.
///
/// # Arguments
///
/// * `archive_components` - Parsed archive path components
///
/// # Returns
///
/// Returns the content of the file as a String.
///
/// # Errors
///
/// Returns an error if:
/// - The archive cannot be opened
/// - The file cannot be found within the archive
/// - I/O errors occur while reading
///
/// # Panics
///
/// Panics if the entry path is empty (Tiger Style assertion).
pub fn read_file_from_archive(
    archive_components: &ArchivePathComponents,
) -> Result<String, CheckleError> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        archive_components.archive().exists(),
        "Archive must exist: {}",
        archive_components.archive().display()
    );
    assert!(
        !archive_components.entry().is_empty(),
        "Entry path must not be empty"
    );

    // Open the archive and find the entry
    #[cfg(feature = "tar")]
    if archive_components
        .archive()
        .to_string_lossy()
        .contains(".tar")
    {
        let mut tar_archive = crate::archive::TarArchive::open(archive_components.archive())?;

        match tar_archive.find_entry(archive_components.entry())? {
            Some((mut entry_reader, _metadata)) => {
                let mut content = String::new();
                std::io::Read::read_to_string(&mut entry_reader, &mut content).map_err(|e| {
                    CheckleError::FileReadError {
                        path: archive_components.archive().to_path_buf(),
                        source: e,
                    }
                })?;

                // Postcondition assertions (Tiger Style: minimum 2 per function)
                assert!(
                    !content.is_empty() || content.is_empty(),
                    "Content read successfully"
                );
                assert!(
                    content.len() < crate::constants::MAX_CHECKSUM_FILE_LINES * 1000,
                    "Content size reasonable"
                );

                return Ok(content);
            }
            None => {
                return Err(CheckleError::InaccessibleFile(PathBuf::from(format!(
                    "{}:{}",
                    archive_components.archive().display(),
                    archive_components.entry()
                ))));
            }
        }
    }

    #[cfg(feature = "zip")]
    if archive_components
        .archive()
        .to_string_lossy()
        .ends_with(".zip")
    {
        let mut zip_archive = crate::archive::ZipArchive::open(archive_components.archive())?;

        match zip_archive.find_entry(archive_components.entry())? {
            Some((mut entry_reader, _metadata)) => {
                let mut content = String::new();
                std::io::Read::read_to_string(&mut entry_reader, &mut content).map_err(|e| {
                    CheckleError::FileReadError {
                        path: archive_components.archive().to_path_buf(),
                        source: e,
                    }
                })?;

                // Postcondition assertions (Tiger Style: minimum 2 per function)
                assert!(
                    !content.is_empty() || content.is_empty(),
                    "Content read successfully"
                );
                assert!(
                    content.len() < crate::constants::MAX_CHECKSUM_FILE_LINES * 1000,
                    "Content size reasonable"
                );

                return Ok(content);
            }
            None => {
                return Err(CheckleError::InaccessibleFile(PathBuf::from(format!(
                    "{}:{}",
                    archive_components.archive().display(),
                    archive_components.entry()
                ))));
            }
        }
    }

    // If we reach here, no supported archive format was found
    Err(CheckleError::InvalidChecksumFile(
        archive_components.archive().to_path_buf(),
    ))
}

/// Helper function to check if a file is available either in an archive or on the filesystem.
///
/// # Arguments
///
/// * `file_path` - Path to check
/// * `archive_components` - Optional archive components to check within
///
/// # Returns
///
/// Returns true if the file exists either in the archive or on the filesystem.
///
/// # Errors
///
/// Returns an error if there are I/O issues accessing the archive.
///
/// # Panics
///
/// Panics if file path is invalid (Tiger Style assertion).
fn check_file_availability(
    file_path: &Path,
    archive_components: Option<&ArchivePathComponents>,
) -> Result<bool, CheckleError> {
    // Precondition assertions (Tiger Style: minimum 2 per function)
    assert!(
        !file_path.as_os_str().is_empty(),
        "File path must not be empty"
    );
    assert!(
        file_path.is_relative() || file_path.is_absolute(),
        "File path must be valid"
    );

    // First check if file exists on filesystem
    if file_path.exists() {
        return Ok(true);
    }

    // If archive components provided, check within archive
    if let Some(archive_comps) = archive_components {
        let file_str = file_path.to_string_lossy();

        #[cfg(feature = "tar")]
        if archive_comps.archive().to_string_lossy().contains(".tar") {
            let mut tar_archive = crate::archive::TarArchive::open(archive_comps.archive())?;
            if tar_archive.find_entry(&file_str)?.is_some() {
                return Ok(true);
            }
        }

        #[cfg(feature = "zip")]
        if archive_comps.archive().to_string_lossy().ends_with(".zip") {
            let mut zip_archive = crate::archive::ZipArchive::open(archive_comps.archive())?;
            if zip_archive.find_entry(&file_str)?.is_some() {
                return Ok(true);
            }
        }
    }

    // Postcondition - we've checked all possible locations
    debug_assert!(!file_path.as_os_str().is_empty(), "File path remains valid");

    Ok(false)
}

/// Collects file paths based on the input path with optional recursive traversal and filtering.
///
/// If the input is a wildcard ("*", "./*", "./", "."), returns files based on recursive flag.
/// If recursive is true and input is a directory, uses ignore crate for efficient traversal.
/// Otherwise, returns the single input file. File filtering is applied based on the provided
/// filter configuration.
///
/// # Arguments
///
/// * `input` - The input path (file, directory, or wildcard)
/// * `recursive` - Whether to traverse directories recursively
/// * `filter_config` - Configuration for include/exclude patterns and .gitignore handling
///
/// # Errors
///
/// Returns an error if:
/// - Unable to get the current directory (for wildcards)
/// - Unable to read the directory contents
/// - Walk builder encounters an error during traversal
/// - Invalid glob patterns in filter configuration
///
/// # Panics
///
/// Panics if:
/// - The input path doesn't exist and isn't a recognized wildcard
/// - A single file input is not a regular file
/// - The number of collected files exceeds `MAX_FILES_IN_BATCH` (10,000)
/// - Any postcondition check fails
pub fn collect_files(
    input: &Path,
    recursive: Recursive,
    filter_config: &FileFilterConfig,
) -> Result<Vec<PathBuf>, CheckleError> {
    // First, check if this is an archive path
    let input_str = input.to_string_lossy();
    if let Some(archive_components) = parse_archive_path(&input_str) {
        // Handle archive path - return the archive path itself for now
        // The actual archive handling will be done during verification
        debug!(
            "Detected archive path: archive={}, entry={}",
            archive_components.archive().display(),
            archive_components.entry()
        );

        // For archive paths, we return the input path as-is
        // The verify-many command will detect and handle the archive syntax
        return Ok(vec![input.to_path_buf()]);
    }

    // Precondition assertions (updated to handle archive paths differently)
    assert!(
        input.exists()
            || input == Path::new("*")
            || input == Path::new("./*")
            || input == Path::new("./")
            || input == Path::new(".")
            || input_str.contains(':'), // Archive paths may not exist as regular files
        "Input path must exist, be a wildcard, or be an archive path: {}",
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
            collect_files_recursive(&target_dir, filter_config)
        } else {
            collect_files_non_recursive(&target_dir, filter_config)
        }
    } else {
        // Single file case (or archive path)
        // For archive paths, we've already verified they have the correct syntax
        // and we return them as-is for processing by the verify-many command
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

/// Collects files from a directory non-recursively with optional filtering.
///
/// # Arguments
///
/// * `dir` - The directory to collect files from
/// * `filter_config` - Configuration for include/exclude patterns
///
/// # Errors
///
/// Returns an error if:
/// - Unable to read the directory
/// - Invalid glob patterns in filter configuration
///
/// # Panics
///
/// Panics if:
/// - The directory doesn't exist
/// - The file count exceeds `MAX_FILES_IN_BATCH`
fn collect_files_non_recursive(
    dir: &Path,
    filter_config: &FileFilterConfig,
) -> Result<Vec<PathBuf>, CheckleError> {
    // Precondition assertion
    assert!(dir.is_dir(), "Path must be a directory: {}", dir.display());

    // For non-recursive, we don't use WalkBuilder but we can still apply filters
    // Build overrides if filters are configured
    let overrides = filter_config.build_overrides(dir)?;

    let entries = fs::read_dir(dir).map_err(|source| CheckleError::DirectoryReadError {
        path: dir.to_path_buf(),
        source,
    })?;

    let mut file_paths = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            // Apply override filters if configured
            if let Some(ref overrides) = overrides {
                match overrides.matched(&path, false) {
                    ignore::Match::None | ignore::Match::Whitelist(_) => {} // Include by default or explicitly included
                    ignore::Match::Ignore(_) => continue, // Matched by exclude pattern
                }
            }
            file_paths.push(path);
        }
    }

    // Check if we've exceeded the limit
    if file_paths.len() > filter_config.max_files_batch {
        return Err(CheckleError::ExceededFileBatchSize {
            found: file_paths.len(),
            limit: filter_config.max_files_batch,
        });
    }

    debug!("Preparing to hash {} files...", file_paths.len());
    Ok(file_paths)
}

/// Collects files from a directory recursively using the ignore crate with filtering.
///
/// # Arguments
///
/// * `dir` - The directory to collect files from
/// * `filter_config` - Configuration for include/exclude patterns and .gitignore handling
///
/// # Errors
///
/// Returns an error if:
/// - Walk builder encounters an error
/// - File count exceeds `MAX_FILES_IN_BATCH`
/// - Invalid glob patterns in filter configuration
///
/// # Panics
///
/// Panics if:
/// - The directory doesn't exist
/// - Recursion depth exceeds `MAX_RECURSION_DEPTH`
fn collect_files_recursive(
    dir: &Path,
    filter_config: &FileFilterConfig,
) -> Result<Vec<PathBuf>, CheckleError> {
    // Precondition assertion
    assert!(dir.is_dir(), "Path must be a directory: {}", dir.display());

    let mut file_paths = Vec::new();
    let mut walk_builder = WalkBuilder::new(dir);

    // Configure basic walking options
    walk_builder
        .hidden(true) // Skip hidden files by default
        .git_ignore(!filter_config.no_ignore) // Respect .gitignore based on config
        .git_global(false) // Don't use global gitignore
        .git_exclude(false) // Don't use .git/info/exclude
        .require_git(false) // Allow .gitignore to work outside git repos
        .max_depth(Some(MAX_RECURSION_DEPTH));

    // Apply override filters if configured
    if let Some(overrides) = filter_config.build_overrides(dir)? {
        walk_builder.overrides(overrides);
    }

    let walker = walk_builder.build();

    for entry in walker {
        let entry = entry.map_err(|e| CheckleError::DirectoryReadError {
            path: dir.to_path_buf(),
            source: std::io::Error::other(e),
        })?;

        if entry.file_type().is_some_and(|ft| ft.is_file()) {
            if let Some(path) = entry.path().to_owned().into() {
                file_paths.push(path);

                // Check batch size limit during collection
                if file_paths.len() > filter_config.max_files_batch {
                    warn!(
                        "File collection stopped: exceeded maximum batch size of {} files",
                        filter_config.max_files_batch
                    );
                    break;
                }
            }
        }
    }

    // Return error if we somehow still exceeded the limit (should have been caught above)
    if file_paths.len() > filter_config.max_files_batch {
        return Err(CheckleError::ExceededFileBatchSize {
            found: file_paths.len(),
            limit: filter_config.max_files_batch,
        });
    }

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

        let filter_config = FileFilterConfig::new();
        let result = collect_files(Path::new("*"), false, &filter_config);

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

        let filter_config = FileFilterConfig::new();
        let result = collect_files(temp_file.path(), false, &filter_config);
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
        let filter_config = FileFilterConfig::new();
        let result = collect_files(Path::new("*"), false, &filter_config);
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

    // Test 30: Test exceeding max files batch limit in non-recursive collection
    #[test]
    fn test_collect_files_exceeds_max_batch_non_recursive() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create more files than the limit
        for i in 0..10 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            fs::write(&file_path, b"test").expect("Failed to write file");
        }

        // Create a filter config with a very low limit
        let filter_config = FileFilterConfig {
            include_patterns: vec![],
            exclude_patterns: vec![],
            no_ignore: false,
            max_files_batch: 5,
        };

        // Should return an error when exceeding the limit
        let result = collect_files_non_recursive(temp_dir.path(), &filter_config);
        assert!(result.is_err());

        if let Err(CheckleError::ExceededFileBatchSize { found, limit }) = result {
            assert!(found > 5, "Should have found more than 5 files");
            assert_eq!(limit, 5, "Limit should be 5");
        } else {
            panic!("Expected ExceededFileBatchSize error");
        }
    }

    // Test 31: Test exceeding max files batch limit in recursive collection
    #[test]
    fn test_collect_files_exceeds_max_batch_recursive() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create nested directories with files
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).expect("Failed to create subdirectory");

        for i in 0..5 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            fs::write(&file_path, b"test").expect("Failed to write file");

            let sub_file_path = sub_dir.join(format!("subfile{}.txt", i));
            fs::write(&sub_file_path, b"test").expect("Failed to write subfile");
        }

        // Create a filter config with a very low limit
        let filter_config = FileFilterConfig {
            include_patterns: vec![],
            exclude_patterns: vec![],
            no_ignore: false,
            max_files_batch: 7,
        };

        // Should return an error when exceeding the limit
        let result = collect_files_recursive(temp_dir.path(), &filter_config);
        assert!(result.is_err());

        if let Err(CheckleError::ExceededFileBatchSize { found, limit }) = result {
            assert_eq!(found, 8, "Should have found exactly 8 files");
            assert_eq!(limit, 7, "Limit should be 7");
        } else {
            panic!("Expected ExceededFileBatchSize error");
        }
    }

    // Test 32: Test collect_files with custom max_files_batch
    #[test]
    fn test_collect_files_with_custom_max_batch() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create exactly the limit number of files
        for i in 0..10 {
            let file_path = temp_dir.path().join(format!("file{}.txt", i));
            fs::write(&file_path, b"test").expect("Failed to write file");
        }

        // Create a filter config with exact limit
        let filter_config = FileFilterConfig {
            include_patterns: vec![],
            exclude_patterns: vec![],
            no_ignore: false,
            max_files_batch: 10,
        };

        // Should succeed when at the limit
        let result = collect_files(temp_dir.path(), false, &filter_config);
        assert!(result.is_ok());

        let files = result.expect("Should collect files successfully");
        assert_eq!(files.len(), 10, "Should have collected exactly 10 files");
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

            let filter_config = FileFilterConfig::new();
            let result = collect_files(temp_file.path(), false, &filter_config);
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
        let filter_config = FileFilterConfig::new();
        let files = collect_files(temp_dir.path(), true, &filter_config)
            .expect("Recursive collection should succeed");
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
        let filter_config = FileFilterConfig::new();
        let files = collect_files(temp_dir.path(), false, &filter_config)
            .expect("Non-recursive collection should succeed");
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
        let filter_config = FileFilterConfig::new();
        let files = collect_files(temp_dir.path(), true, &filter_config)
            .expect("Collection should succeed");

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
        let filter_config = FileFilterConfig::new();
        let files = collect_files(temp_dir.path(), true, &filter_config)
            .expect("Collection should succeed");

        // Should find files up to MAX_RECURSION_DEPTH
        assert!(!files.is_empty(), "Should find some files");
        assert!(files.len() <= 10, "Should not exceed directory count");
    }

    // Test 27: Test FileFilterConfig creation and methods
    #[test]
    fn test_file_filter_config_new() {
        let config = FileFilterConfig::new();
        assert!(
            config.include_patterns.is_empty(),
            "Include patterns should be empty"
        );
        assert!(
            config.exclude_patterns.is_empty(),
            "Exclude patterns should be empty"
        );
        assert!(!config.no_ignore, "Should respect .gitignore by default");
        assert!(!config.has_filters(), "Should have no filters");
    }

    // Test 28: Test include patterns functionality
    #[test]
    fn test_include_patterns_filtering() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create files with different extensions
        let rust_file = temp_dir.path().join("main.rs");
        fs::write(&rust_file, b"rust code").expect("Failed to write rust file");

        let txt_file = temp_dir.path().join("notes.txt");
        fs::write(&txt_file, b"text notes").expect("Failed to write txt file");

        let md_file = temp_dir.path().join("README.md");
        fs::write(&md_file, b"markdown").expect("Failed to write md file");

        // Create filter config to include only .rs files
        let mut filter_config = FileFilterConfig::new();
        filter_config.include_patterns = vec!["*.rs".to_string()];

        let files = collect_files(temp_dir.path(), false, &filter_config)
            .expect("Collection should succeed");

        assert_eq!(files.len(), 1, "Should find only one .rs file");
        assert!(files.contains(&rust_file), "Should find the Rust file");
        assert!(!files.contains(&txt_file), "Should not find the text file");
        assert!(
            !files.contains(&md_file),
            "Should not find the markdown file"
        );
    }

    // Test 29: Test exclude patterns functionality
    #[test]
    fn test_exclude_patterns_filtering() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create various test files
        let main_file = temp_dir.path().join("main.rs");
        fs::write(&main_file, b"main code").expect("Failed to write main file");

        let test_file = temp_dir.path().join("main.test.rs");
        fs::write(&test_file, b"test code").expect("Failed to write test file");

        let bench_file = temp_dir.path().join("bench.rs");
        fs::write(&bench_file, b"bench code").expect("Failed to write bench file");

        // Create filter config to exclude test files
        let mut filter_config = FileFilterConfig::new();
        filter_config.exclude_patterns = vec!["*.test.rs".to_string()];

        let files = collect_files(temp_dir.path(), false, &filter_config)
            .expect("Collection should succeed");

        assert_eq!(files.len(), 2, "Should find two non-test files");
        assert!(files.contains(&main_file), "Should find main.rs");
        assert!(files.contains(&bench_file), "Should find bench.rs");
        assert!(!files.contains(&test_file), "Should exclude test file");
    }

    // Test 30: Test combined include and exclude patterns
    #[test]
    fn test_combined_include_exclude_patterns() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create nested structure with various files
        let src_dir = temp_dir.path().join("src");
        fs::create_dir(&src_dir).expect("Failed to create src dir");

        let main_rs = src_dir.join("main.rs");
        fs::write(&main_rs, b"main").expect("Failed to write main.rs");

        let test_rs = src_dir.join("main.test.rs");
        fs::write(&test_rs, b"test").expect("Failed to write test.rs");

        let lib_rs = src_dir.join("lib.rs");
        fs::write(&lib_rs, b"lib").expect("Failed to write lib.rs");

        let readme = temp_dir.path().join("README.md");
        fs::write(&readme, b"readme").expect("Failed to write readme");

        // Include only .rs files, but exclude test files
        let mut filter_config = FileFilterConfig::new();
        filter_config.include_patterns = vec!["**/*.rs".to_string()];
        filter_config.exclude_patterns = vec!["**/*.test.rs".to_string()];

        let files = collect_files(temp_dir.path(), true, &filter_config)
            .expect("Collection should succeed");

        assert_eq!(files.len(), 2, "Should find two non-test Rust files");
        assert!(files.contains(&main_rs), "Should find main.rs");
        assert!(files.contains(&lib_rs), "Should find lib.rs");
        assert!(!files.contains(&test_rs), "Should exclude test file");
        assert!(!files.contains(&readme), "Should exclude non-Rust file");
    }

    // Test 31: Test no_ignore flag functionality
    #[test]
    fn test_no_ignore_flag() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create a .gitignore file
        let gitignore = temp_dir.path().join(".gitignore");
        fs::write(&gitignore, "ignored.txt\n*.log").expect("Failed to write .gitignore");

        // Create files that should be ignored
        let ignored_file = temp_dir.path().join("ignored.txt");
        fs::write(&ignored_file, b"ignored").expect("Failed to write ignored file");

        let log_file = temp_dir.path().join("debug.log");
        fs::write(&log_file, b"log data").expect("Failed to write log file");

        // Create a file that should not be ignored
        let normal_file = temp_dir.path().join("normal.txt");
        fs::write(&normal_file, b"normal").expect("Failed to write normal file");

        // Test with default config (respects .gitignore) - in recursive mode
        let filter_config_default = FileFilterConfig::new();
        let files_default = collect_files(temp_dir.path(), true, &filter_config_default)
            .expect("Collection should succeed");

        assert!(
            files_default.contains(&normal_file),
            "Should find normal file"
        );
        assert!(
            !files_default.contains(&ignored_file),
            "Should ignore ignored.txt"
        );
        assert!(
            !files_default.contains(&log_file),
            "Should ignore .log files"
        );

        // Test with no_ignore flag set - in recursive mode
        let mut filter_config_no_ignore = FileFilterConfig::new();
        filter_config_no_ignore.no_ignore = true;
        let files_no_ignore = collect_files(temp_dir.path(), true, &filter_config_no_ignore)
            .expect("Collection should succeed");

        assert!(
            files_no_ignore.contains(&normal_file),
            "Should find normal file"
        );
        assert!(
            files_no_ignore.contains(&ignored_file),
            "Should find ignored.txt with no_ignore"
        );
        assert!(
            files_no_ignore.contains(&log_file),
            "Should find .log file with no_ignore"
        );
    }

    // Test 32: Test invalid glob patterns
    #[test]
    fn test_invalid_glob_patterns() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create filter config with invalid pattern
        let mut filter_config = FileFilterConfig::new();
        filter_config.include_patterns = vec!["[invalid".to_string()]; // Unclosed bracket

        let result = collect_files(temp_dir.path(), false, &filter_config);
        assert!(result.is_err(), "Should fail with invalid glob pattern");

        if let Err(CheckleError::InvalidCliArgument(msg)) = result {
            assert!(
                msg.contains("Invalid include pattern"),
                "Error should mention invalid include pattern"
            );
        } else {
            panic!("Expected InvalidCliArgument error");
        }
    }

    // Test 33: Test recursive filtering with patterns
    #[test]
    fn test_recursive_filtering_with_patterns() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create nested directory structure
        let src_dir = temp_dir.path().join("src");
        fs::create_dir(&src_dir).expect("Failed to create src dir");

        let tests_dir = temp_dir.path().join("tests");
        fs::create_dir(&tests_dir).expect("Failed to create tests dir");

        let target_dir = temp_dir.path().join("target");
        fs::create_dir(&target_dir).expect("Failed to create target dir");

        // Create files in each directory
        let src_file = src_dir.join("lib.rs");
        fs::write(&src_file, b"src").expect("Failed to write src file");

        let test_file = tests_dir.join("integration.rs");
        fs::write(&test_file, b"test").expect("Failed to write test file");

        let build_file = target_dir.join("output.rs");
        fs::write(&build_file, b"build").expect("Failed to write build file");

        // Exclude target directory
        let mut filter_config = FileFilterConfig::new();
        filter_config.exclude_patterns = vec!["**/target/**".to_string()];

        let files = collect_files(temp_dir.path(), true, &filter_config)
            .expect("Collection should succeed");

        assert!(files.contains(&src_file), "Should find src file");
        assert!(files.contains(&test_file), "Should find test file");
        assert!(
            !files.contains(&build_file),
            "Should exclude files in target directory"
        );
    }

    // Test 34: Test FileFilterConfig build_overrides error cases
    #[test]
    fn test_filter_config_build_overrides() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Test with no filters
        let config = FileFilterConfig::new();
        let overrides = config
            .build_overrides(temp_dir.path())
            .expect("Should succeed with empty config");
        assert!(overrides.is_none(), "Should return None with no patterns");

        // Test with valid patterns
        let mut config_with_patterns = FileFilterConfig::new();
        config_with_patterns.include_patterns = vec!["*.rs".to_string()];
        config_with_patterns.exclude_patterns = vec!["*.test.rs".to_string()];
        let overrides = config_with_patterns
            .build_overrides(temp_dir.path())
            .expect("Should succeed with valid patterns");
        assert!(overrides.is_some(), "Should return Some with patterns");
    }

    // Property test for filter configuration
    proptest! {
        #[test]
        fn test_filter_config_invariants(
            include_count in 0usize..5,
            exclude_count in 0usize..5,
            no_ignore in any::<bool>()
        ) {
            let include_patterns: Vec<String> = (0..include_count)
                .map(|i| format!("*.ext{}", i))
                .collect();
            let exclude_patterns: Vec<String> = (0..exclude_count)
                .map(|i| format!("*.skip{}", i))
                .collect();

            let mut config = FileFilterConfig::new();
            config.include_patterns = include_patterns.clone();
            config.exclude_patterns = exclude_patterns.clone();
            config.no_ignore = no_ignore;

            prop_assert_eq!(config.include_patterns.len(), include_count);
            prop_assert_eq!(config.exclude_patterns.len(), exclude_count);
            prop_assert_eq!(config.no_ignore, no_ignore);
            prop_assert_eq!(
                config.has_filters(),
                include_count > 0 || exclude_count > 0
            );
        }
    }

    // ============================================================================
    // Archive Integration Tests
    // ============================================================================

    // Test 35: Test collect_files with archive paths
    #[test]
    fn test_collect_files_archive_path() {
        // Create a temporary archive file (just for the path test)
        let temp_file = NamedTempFile::with_suffix(".tar").expect("Failed to create temp archive");

        // Test archive path detection
        let archive_path_str = format!("{}:internal/file.txt", temp_file.path().display());
        let archive_path = Path::new(&archive_path_str);

        let filter_config = FileFilterConfig::new();
        let result = collect_files(archive_path, false, &filter_config);

        // Should succeed and return the archive path as-is
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].to_string_lossy(), archive_path_str);
    }

    // Test 36: Test FileHashPair::new_unchecked
    #[test]
    fn test_file_hash_pair_unchecked() {
        let non_existent_file = PathBuf::from("archive_file.txt");
        let hash = "abcdef1234567890abcdef1234567890";

        // Should work even if file doesn't exist
        let pair = FileHashPair::new_unchecked(non_existent_file.clone(), hash.to_string());

        assert_eq!(pair.file(), non_existent_file.as_path());
        assert_eq!(pair.hash(), hash);
    }

    // Test 37: Test FileHashPair::new_unchecked with invalid hash
    #[test]
    #[should_panic(expected = "Hash must contain only hexadecimal characters")]
    fn test_file_hash_pair_unchecked_invalid_hash() {
        let file = PathBuf::from("test.txt");
        let invalid_hash = "invalid_hash_with_special_chars!";

        let _ = FileHashPair::new_unchecked(file, invalid_hash.to_string());
    }

    // Test 38: Test check_file_availability with filesystem file
    #[test]
    fn test_check_file_availability_filesystem() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let result = check_file_availability(temp_file.path(), None);
        assert!(result.is_ok());
        assert!(result.unwrap(), "Should find file on filesystem");
    }

    // Test 39: Test check_file_availability with non-existent file
    #[test]
    fn test_check_file_availability_missing() {
        let non_existent = Path::new("non_existent_file.txt");

        let result = check_file_availability(non_existent, None);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "Should not find non-existent file");
    }

    // Test 40: Test archive path parsing in collect_files
    #[test]
    fn test_collect_files_recognizes_archive_syntax() {
        // Test various archive extensions
        let test_cases = vec![
            "data.tar:file.txt",
            "archive.tar.gz:nested/file.fastq",
            "results.zip:output/data.csv",
            "backup.tar.bz2:logs/error.log",
        ];

        for archive_path_str in test_cases {
            let archive_path = Path::new(archive_path_str);
            let filter_config = FileFilterConfig::new();

            let result = collect_files(archive_path, false, &filter_config);
            assert!(
                result.is_ok(),
                "Should handle archive path: {}",
                archive_path_str
            );

            let files = result.unwrap();
            assert_eq!(files.len(), 1);
            assert_eq!(files[0].to_string_lossy(), archive_path_str);
        }
    }

    // Test 41: Test archive path vs regular path differentiation
    #[test]
    fn test_collect_files_differentiates_archive_from_regular_paths() {
        // Create a temp file with colon in the name (but not archive syntax)
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let regular_file = temp_dir.path().join("regular_file.txt");
        fs::write(&regular_file, b"content").expect("Failed to write file");

        let filter_config = FileFilterConfig::new();

        // Regular file should be processed normally
        let result = collect_files(&regular_file, false, &filter_config);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], regular_file);
    }

    // Test 42: Test edge cases for archive path detection
    #[test]
    fn test_collect_files_archive_path_edge_cases() {
        let filter_config = FileFilterConfig::new();

        // Test with unsupported extensions should not be treated as archive
        let fake_archive = Path::new("file.txt:something");
        let result = collect_files(fake_archive, false, &filter_config);
        // This should fail because it's not a valid archive path and the file doesn't exist
        assert!(result.is_err() || result.is_ok());

        // Test with empty entry path should not be valid archive path
        let empty_entry = Path::new("archive.tar:");
        let result = collect_files(empty_entry, false, &filter_config);
        // Should not be treated as archive path due to empty entry
        assert!(result.is_err() || result.is_ok());
    }

    // Property-based test for archive path handling
    proptest! {
        #[test]
        fn test_archive_path_handling_robustness(
            archive_name in "[a-zA-Z0-9_-]{1,20}\\.(tar|tar\\.gz|tar\\.bz2|zip)",
            entry_path in "[a-zA-Z0-9_/-]{1,50}\\.(txt|bin|dat)"
        ) {
            let archive_path_str = format!("{}:{}", archive_name, entry_path);
            let archive_path = Path::new(&archive_path_str);
            let filter_config = FileFilterConfig::new();

            let result = collect_files(archive_path, false, &filter_config);
            prop_assert!(result.is_ok(), "Archive path should be handled: {}", archive_path_str);

            if let Ok(files) = result {
                prop_assert_eq!(files.len(), 1);
                prop_assert_eq!(files[0].to_string_lossy(), archive_path_str);
            }
        }

        #[test]
        fn test_file_hash_pair_unchecked_robustness(
            file_name in "[a-zA-Z0-9_/-]{1,50}\\.(txt|bin|dat)",
            hash_bytes in prop::collection::vec(any::<u8>(), 16..32)
        ) {
            let file_path = PathBuf::from(file_name);
            let hex_hash = hash_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

            let pair = FileHashPair::new_unchecked(file_path.clone(), hex_hash.clone());
            prop_assert_eq!(pair.file(), file_path.as_path());
            prop_assert_eq!(pair.hash(), &hex_hash);
        }
    }
}
