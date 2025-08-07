#![allow(clippy::print_stdout)]

use crate::{
    archive::Archive,
    archive_path,
    cli::{AbsolutePaths, NoIgnore, NoProgress, OutputFormat, PerFileMode, PrettyPrint, Recursive},
    errors::{CheckleError, Result},
    io::{FileFilterConfig, FileHashPair, PathDisplayMode, collect_files, format_path_for_display},
    prelude::*,
    prettyprint::{FileHashPairWithMetadata, convert_to_basic_pairs, display_pretty_table},
    progress::ProgressManager,
};
use log::{debug, info};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::{
    fmt::Write as FmtWrite,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Configuration for the hash command.
#[derive(Debug, Clone)]
pub struct HashConfig<'a> {
    pub input_file: &'a Path,
    pub recursive: Recursive,
    pub hash_output: Option<&'a Path>,
    pub format: Option<OutputFormat>,
    pub pretty: PrettyPrint,
    pub per_file: PerFileMode,
    pub no_progress: NoProgress,
    pub include: &'a [String],
    pub exclude: &'a [String],
    pub no_ignore: NoIgnore,
    pub algo: HashingAlgo,
    pub chunk_size_kb: usize,
    pub parallel_readers: usize,
    pub max_files_batch: usize,
    pub absolute_paths: AbsolutePaths,
}

impl HashConfig<'_> {
    /// Validate the configuration.
    fn validate(&self) -> Result<()> {
        // Validate chunk size (must be reasonable)
        if self.chunk_size_kb > 1_048_576 {
            return Err(CheckleError::ConfigError(
                "Chunk size must be between 1 KB and 1 GB".to_string(),
            ));
        }

        // Validate parallel readers (must be reasonable)
        if self.parallel_readers > 1024 {
            return Err(CheckleError::ConfigError(
                "Parallel readers must be 1024 or less".to_string(),
            ));
        }

        // Validate max files batch
        if self.max_files_batch == 0 {
            return Err(CheckleError::ConfigError(
                "Max files batch must be at least 1".to_string(),
            ));
        }

        // Validate input file exists (allow wildcards)
        if !self.input_file.exists()
            && !is_archive_path(self.input_file)
            && !is_wildcard_pattern(self.input_file)
        {
            return Err(CheckleError::InaccessibleFile(
                self.input_file.to_path_buf(),
            ));
        }

        Ok(())
    }
}

/// Execute the hash command to generate hashes for one or more files.
///
/// This function supports:
/// - Recursive directory traversal
/// - Archive traversal (when --recursive is used with an archive)
/// - Archive entry hashing (specific files within archives)
/// - Multiple output formats (text, CSV, JSON)
/// - Pretty table display
/// - Per-file hash writing
/// - Progress tracking
///
/// # Arguments
///
/// * `input_file` - Path to file/directory/archive to hash
/// * `recursive` - Whether to traverse directories/archives recursively
/// * `hash_output` - Optional path to write hash output file
/// * `format` - Optional output format (auto-detected if not specified)
/// * `pretty` - Whether to display results in a pretty table
/// * `per_file` - Whether to write individual hash files
/// * `no_progress` - Whether to disable progress display
/// * `include` - Include patterns for filtering
/// * `exclude` - Exclude patterns for filtering
/// * `no_ignore` - Whether to ignore .gitignore rules
/// * `algo` - Hashing algorithm to use
/// * `chunk_size_kb` - Chunk size in KB for hashing
/// * `parallel_readers` - Number of parallel readers for hashing  
/// * `max_files_batch` - Maximum number of files to process in batch
///
/// # Errors
///
/// Returns an error if file access or hashing fails
// TODO: Refactor the caller (main.rs) to create HashConfig directly instead of passing all parameters
#[allow(clippy::too_many_arguments)]
pub fn execute(
    input_file: &Path,
    recursive: Recursive,
    hash_output: Option<&Path>,
    format: Option<OutputFormat>,
    pretty: PrettyPrint,
    per_file: PerFileMode,
    no_progress: NoProgress,
    include: &[String],
    exclude: &[String],
    no_ignore: NoIgnore,
    algo: HashingAlgo,
    chunk_size_kb: usize,
    parallel_readers: usize,
    max_files_batch: usize,
    absolute_paths: AbsolutePaths,
) -> Result<()> {
    let config = HashConfig {
        input_file,
        recursive,
        hash_output,
        format,
        pretty,
        per_file,
        no_progress,
        include,
        exclude,
        no_ignore,
        algo,
        chunk_size_kb,
        parallel_readers,
        max_files_batch,
        absolute_paths,
    };

    config.execute_hash()
}

impl HashConfig<'_> {
    /// Execute the hash operation with this configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration validation fails
    /// - File access fails
    /// - Hash computation fails
    pub fn execute_hash(&self) -> Result<()> {
        // Validate configuration first
        self.validate()?;

        // Create filter configuration from CLI arguments
        let filter_config = FileFilterConfig {
            include_patterns: self.include.to_vec(),
            exclude_patterns: self.exclude.to_vec(),
            no_ignore: self.no_ignore,
            max_files_batch: self.max_files_batch,
        };

        // Check if input_file is an archive path (contains ':') - if so, handle it as archive introspection
        let input_file_str = self.input_file.to_string_lossy();
        if let Some(_archive_components) = archive_path::parse_archive_path(&input_file_str) {
            return self.hash_single_archive_entry();
        }

        // Regular file/directory hashing (archives are treated as regular files unless ':' syntax is used)
        self.hash_regular_files(&filter_config)
    }

    /// Hash archive entries using archive path syntax with pattern support.
    #[allow(clippy::too_many_lines)] // Function orchestrates several operations
    fn hash_single_archive_entry(&self) -> Result<()> {
        // Parse archive path components
        let input_file_str = self.input_file.to_string_lossy();
        let archive_components =
            archive_path::parse_archive_path(&input_file_str).ok_or_else(|| {
                CheckleError::InvalidCliArgument(format!(
                    "Invalid archive path syntax: {input_file_str}"
                ))
            })?;

        // Open archive for pattern matching
        let mut archive = Archive::open(archive_components.archive())?;

        // Get matching entries based on pattern
        let matching_entries = archive.list_matching_entries(archive_components.pattern())?;

        // For specific file patterns, if nothing matches, return an error
        if matching_entries.is_empty()
            && let archive_path::ArchivePattern::SpecificFile(path) = archive_components.pattern()
        {
            return Err(CheckleError::ArchiveEntryNotFound {
                archive: archive_components.archive().to_path_buf(),
                entry: path.clone(),
            });
        }
        assert!(
            matching_entries.len() <= crate::constants::MAX_FILES_IN_BATCH,
            "Matching entries {} should not exceed batch limit {}",
            matching_entries.len(),
            crate::constants::MAX_FILES_IN_BATCH
        );

        if matching_entries.is_empty() {
            println!(
                "No entries found matching pattern: {}",
                archive_components.pattern().as_str()
            );
            return Ok(());
        }

        // Hash each matching entry
        let mut file_hash_pairs = Vec::with_capacity(matching_entries.len());
        let chunk_size_kb = convert_chunk_size_kb(self.chunk_size_kb);

        // Initialize progress tracking if enabled
        let show_progress = !self.no_progress && matching_entries.len() > 1;
        let progress_manager = ProgressManager::new(show_progress, matching_entries.len());

        for entry_path in &matching_entries {
            // Create archive path for this specific entry
            let archive_entry_path =
                format!("{}:{}", archive_components.archive().display(), entry_path);
            let entry_path_buf = PathBuf::from(&archive_entry_path);

            // Create data source for this entry
            let source = create_data_source_from_path(&entry_path_buf)?;

            // Create per-file progress if the file is large enough
            let file_size = 0; // We don't know the size beforehand for archive entries
            let file_progress =
                progress_manager.create_file_progress(&archive_entry_path, file_size);

            let hash = match file_progress {
                Some(progress) => {
                    // With progress callback
                    source.hash(
                        self.algo,
                        chunk_size_kb,
                        self.parallel_readers,
                        Some(move |bytes_read| {
                            progress.update(bytes_read);
                        }),
                    )
                }
                None => {
                    // Without progress callback
                    source.hash(
                        self.algo,
                        chunk_size_kb,
                        self.parallel_readers,
                        None::<fn(u64)>,
                    )
                }
            }?;

            // Create result with archive metadata (use fallback since it's an archive entry)
            let result =
                FileHashPairWithMetadata::new_with_fallback(entry_path_buf.clone(), hash.clone())?;

            file_hash_pairs.push(result);

            // Handle per-file output if requested
            if self.per_file {
                write_per_file_hash(&entry_path_buf, &hash, self.algo)?;
            }
        }

        // Progress tracking finishes automatically

        // Postcondition assertions (Tiger Style: minimum 2 per function)
        assert_eq!(
            file_hash_pairs.len(),
            matching_entries.len(),
            "Hash result count should match matching entries count"
        );
        assert!(
            file_hash_pairs.iter().all(|pair| !pair.hash().is_empty()),
            "All hash results should be non-empty"
        );

        // Handle output (per-file already handled in loop above)
        if !self.per_file {
            if let Some(output_path) = self.hash_output {
                // Write to output file
                let output_format = self.format.unwrap_or(OutputFormat::Text);
                let path_mode = PathDisplayMode::from_flag(self.absolute_paths);
                let formatted_output = format_output_with_pretty(
                    &convert_to_basic_pairs(file_hash_pairs.clone()),
                    output_format,
                    self.pretty,
                    path_mode,
                );
                std::fs::write(output_path, formatted_output).map_err(|e| {
                    CheckleError::FileOpenError {
                        path: output_path.to_path_buf(),
                        source: e,
                    }
                })?;
            } else if self.pretty {
                // Pretty table output
                display_pretty_table(&file_hash_pairs)?;
            } else {
                // Standard output
                let output_format = self.format.unwrap_or(OutputFormat::Text);
                let path_mode = PathDisplayMode::from_flag(self.absolute_paths);
                let formatted_output = format_output_with_pretty(
                    &convert_to_basic_pairs(file_hash_pairs),
                    output_format,
                    false,
                    path_mode,
                );
                print!("{formatted_output}");
            }
        }

        Ok(())
    }

    /// Hash regular files/directories (not archives).
    fn hash_regular_files(&self, filter_config: &FileFilterConfig) -> Result<()> {
        let files = collect_files(self.input_file, self.recursive, filter_config)?;

        // Create progress manager based on no_progress flag
        let show_progress = !self.no_progress;
        let progress_manager = ProgressManager::new(show_progress, files.len());

        // Clone necessary values for use in parallel iterator
        let progress_manager_clone = progress_manager.clone();
        let algo = self.algo;
        let chunk_size_kb = self.chunk_size_kb;
        let parallel_readers = self.parallel_readers;

        let file_hash_pairs = files
            .into_par_iter()
            .map(move |file| -> Result<FileHashPairWithMetadata> {
                // Get file metadata for both progress tracking and pretty output
                let (file_size, metadata_opt) = match std::fs::metadata(&file) {
                    Ok(metadata) => (metadata.len(), Some(metadata)),
                    Err(_) => (0, None), // If we can't get metadata, use fallback
                };

                // Create per-file progress bar if the file is large enough
                let file_progress = progress_manager_clone
                    .create_file_progress(file.to_string_lossy().as_ref(), file_size);

                let result = match algo {
                    HashingAlgo::Md5 => {
                        let mut hasher = Hasher::new_md5(&file);

                        // Configure hasher with cloned values
                        hasher = hasher.with_chunk_size(chunk_size_kb * 1024)?;
                        if parallel_readers > 0 {
                            hasher = hasher.with_parallel_readers(parallel_readers);
                        }

                        // Add progress callback if we have a progress bar
                        if let Some(progress) = file_progress {
                            hasher = hasher.with_progress_callback(Box::new(move |bytes_read| {
                                progress.update(bytes_read);
                            }));
                        }

                        let hash = hasher.find_root_hash()?;
                        match metadata_opt {
                            Some(metadata) => FileHashPairWithMetadata::new(file, hash, &metadata),
                            None => FileHashPairWithMetadata::new_with_fallback(file, hash),
                        }
                    }
                    HashingAlgo::Sha2 => {
                        let mut hasher = Hasher::new_sha2(&file);

                        // Configure hasher with cloned values
                        hasher = hasher.with_chunk_size(chunk_size_kb * 1024)?;
                        if parallel_readers > 0 {
                            hasher = hasher.with_parallel_readers(parallel_readers);
                        }

                        // Add progress callback if we have a progress bar
                        if let Some(progress) = file_progress {
                            hasher = hasher.with_progress_callback(Box::new(move |bytes_read| {
                                progress.update(bytes_read);
                            }));
                        }

                        let hash = hasher.find_root_hash()?;
                        match metadata_opt {
                            Some(metadata) => FileHashPairWithMetadata::new(file, hash, &metadata),
                            None => FileHashPairWithMetadata::new_with_fallback(file, hash),
                        }
                    }
                };

                // Update overall progress
                progress_manager_clone.inc_overall();

                result
            })
            .collect::<Result<Vec<_>>>()?;

        // Finish progress display
        progress_manager.finish_with_message(&format!("Hashed {} files", file_hash_pairs.len()));

        debug!("Finished hashing {} file(s).", file_hash_pairs.len());

        // Display pretty table to stderr if requested
        if self.pretty {
            display_pretty_table(&file_hash_pairs)?;
        }

        // Convert enhanced pairs to basic pairs for backward compatibility
        let basic_file_hash_pairs = convert_to_basic_pairs(file_hash_pairs);

        // Determine output format
        let output_format = if let Some(fmt) = self.format {
            // Explicit format provided via --format flag
            fmt
        } else if let Some(output_path) = self.hash_output {
            // Auto-detect from file extension
            OutputFormat::detect_from_path(output_path)
        } else {
            // Default to text format
            OutputFormat::Text
        };

        // Handle file output and optional stdout output
        handle_hash_output(
            &basic_file_hash_pairs,
            self.per_file,
            self.hash_output,
            output_format,
            self.pretty,
            self.absolute_paths,
            self.algo,
        )
    }
}

/// Handle output of hash results.
#[allow(clippy::print_stdout)]
fn handle_hash_output(
    file_hash_pairs: &[FileHashPair],
    per_file: PerFileMode,
    hash_output: Option<&Path>,
    output_format: OutputFormat,
    pretty: PrettyPrint,
    absolute_paths: AbsolutePaths,
    algo: HashingAlgo,
) -> Result<()> {
    if per_file {
        // Write hash files alongside each source file
        for file_hash_pair in file_hash_pairs {
            write_per_file_hash(file_hash_pair.file(), file_hash_pair.hash(), algo)?;
        }

        info!("Created {} hash files", file_hash_pairs.len());

        // Still output to stdout for visibility unless pretty mode
        if !pretty {
            println!(
                "{}",
                format_output_with_pretty(
                    file_hash_pairs,
                    output_format,
                    pretty,
                    PathDisplayMode::from_flag(absolute_paths)
                )
            );
        }
    } else if let Some(output_path) = hash_output {
        // Write to specified file
        let checksum_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    CheckleError::OutputFileExists {
                        path: output_path.to_path_buf(),
                    }
                } else {
                    CheckleError::FileOpenError {
                        path: output_path.to_path_buf(),
                        source: e,
                    }
                }
            })?;

        let mut writer = BufWriter::new(checksum_file);
        let formatted_output = format_output_with_pretty(
            file_hash_pairs,
            output_format,
            pretty,
            PathDisplayMode::from_flag(absolute_paths),
        );
        debug!(
            "Writing {} formatted hash records...",
            file_hash_pairs.len()
        );
        write!(writer, "{formatted_output}").map_err(|e| CheckleError::FileOpenError {
            path: output_path.to_path_buf(),
            source: e,
        })?;
        writer.flush().map_err(|e| CheckleError::FileOpenError {
            path: output_path.to_path_buf(),
            source: e,
        })?;

        info!("Hashes written to: {}", output_path.display());

        // Do NOT print to stdout when output file is specified
    } else {
        // Default behavior: output only to stdout (no file creation)
        let formatted_output = format_output_with_pretty(
            file_hash_pairs,
            output_format,
            pretty,
            PathDisplayMode::from_flag(absolute_paths),
        );
        debug!(
            "Writing {} formatted hash records to stdout...",
            file_hash_pairs.len()
        );

        // Print to stdout unless pretty mode is enabled (which outputs to stderr instead)
        if !pretty {
            println!("{formatted_output}");
        }
    }
    Ok(())
}

// Helper functions

/// Check if a path contains archive syntax (e.g., archive.tar:entry).
#[inline]
fn is_archive_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    archive_path::parse_archive_path(&path_str).is_some()
}

/// Check if a path is a wildcard pattern.
#[inline]
fn is_wildcard_pattern(path: &Path) -> bool {
    path == Path::new("*")
        || path == Path::new("./*")
        || path == Path::new("./")
        || path == Path::new(".")
}

/// Create a `DataSource` from a path that might contain archive syntax.
fn create_data_source_from_path(file_path: &Path) -> Result<crate::data_source::DataSource> {
    use crate::data_source::DataSource;

    let path_str = file_path.to_string_lossy();

    // Check if this is an archive path
    if let Some(archive_components) = archive_path::parse_archive_path(&path_str) {
        DataSource::from_archive(&archive_components)
    } else {
        // Regular filesystem path
        if file_path.exists() {
            Ok(DataSource::from_path(file_path.to_path_buf()))
        } else {
            Err(CheckleError::InaccessibleFile(file_path.to_path_buf()))
        }
    }
}

/// Convert chunk size from usize to u16.
///
/// Returns 0 if input is 0, otherwise clamps to 1024 if the value is too large for u16.
#[inline]
fn convert_chunk_size_kb(chunk_size_kb: usize) -> u16 {
    if chunk_size_kb == 0 {
        0
    } else {
        u16::try_from(chunk_size_kb).unwrap_or(1024) // Default to 1024 if too large
    }
}

/// Write hash to per-file hash file.
fn write_per_file_hash(file_path: &Path, hash: &str, algorithm: HashingAlgo) -> Result<()> {
    let hash_file_path = get_per_file_hash_path(file_path, algorithm);

    // Write in standard format: "hash  filename" (two spaces)
    // This matches the output format of md5sum and sha256sum tools
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let content = format!("{hash}  {filename}\n");

    std::fs::write(&hash_file_path, content).map_err(|e| CheckleError::FileOpenError {
        path: hash_file_path.clone(),
        source: e,
    })?;

    debug!("Wrote hash to: {}", hash_file_path.display());
    Ok(())
}

/// Get the per-file hash filename.
///
/// Appends the appropriate extension (.md5 or .sha256) to the file path.
#[inline]
fn get_per_file_hash_path(file_path: &Path, algorithm: HashingAlgo) -> PathBuf {
    let extension = match algorithm {
        HashingAlgo::Md5 => "md5",
        HashingAlgo::Sha2 => "sha256",
    };

    let mut hash_path = file_path.to_path_buf();
    let current_name = hash_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let new_name = format!("{current_name}.{extension}");
    hash_path.set_file_name(new_name);
    hash_path
}

/// Format output with optional pretty printing.
fn format_output_with_pretty(
    file_hash_pairs: &[FileHashPair],
    format: OutputFormat,
    pretty: bool,
    path_mode: PathDisplayMode,
) -> String {
    match format {
        OutputFormat::Text => {
            // Tab-delimited format: hash\tfile_path
            let lines: Vec<String> = file_hash_pairs
                .iter()
                .map(|file| {
                    format!(
                        "{}\t{}",
                        file.hash(),
                        format_path_for_display(file.file(), path_mode)
                    )
                })
                .collect();
            lines.join("\n")
        }
        OutputFormat::Csv => {
            // CSV format with proper escaping
            let mut output = String::from("hash,filepath\n"); // Header
            for file in file_hash_pairs {
                let hash = file.hash();
                let filepath = format_path_for_display(file.file(), path_mode);

                // Escape CSV fields if they contain special characters
                let escaped_filepath = if filepath.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", filepath.replace('"', "\"\""))
                } else {
                    filepath.to_string()
                };

                writeln!(&mut output, "{hash},{escaped_filepath}")
                    .expect("Writing to String should never fail");
            }
            output.trim_end().to_string() // Remove trailing newline
        }
        OutputFormat::Json => {
            // For now, create JSON manually to avoid adding serde dependency
            // This will be replaced with proper serde serialization
            if pretty {
                format_hash_json_pretty(file_hash_pairs, path_mode)
            } else {
                format_hash_json_compact(file_hash_pairs, path_mode)
            }
        }
    }
}

fn format_hash_json_compact(
    file_hash_pairs: &[FileHashPair],
    path_mode: PathDisplayMode,
) -> String {
    let mut json_objects = Vec::new();
    for file in file_hash_pairs {
        let hash = file.hash();
        let filepath = format_path_for_display(file.file(), path_mode);
        let escaped_filepath = escape_json_string(&filepath);
        json_objects.push(format!(
            "{{\"hash\":\"{hash}\",\"filepath\":\"{escaped_filepath}\"}}"
        ));
    }
    format!("[{}]", json_objects.join(","))
}

fn format_hash_json_pretty(file_hash_pairs: &[FileHashPair], path_mode: PathDisplayMode) -> String {
    let mut output = String::from("[\n");
    for (i, file) in file_hash_pairs.iter().enumerate() {
        let hash = file.hash();
        let filepath = format_path_for_display(file.file(), path_mode);
        let escaped_filepath = escape_json_string(&filepath);

        output.push_str("  {\n");
        writeln!(&mut output, "    \"hash\": \"{hash}\",").expect("Failed to write hash to output");
        writeln!(&mut output, "    \"filepath\": \"{escaped_filepath}\"")
            .expect("Failed to write filepath to output");

        if i == file_hash_pairs.len() - 1 {
            output.push_str("  }\n");
        } else {
            output.push_str("  },\n");
        }
    }
    output.push_str("]\n");
    output
}

/// Escape JSON strings consistently.
///
/// Escapes backslashes, quotes, newlines, carriage returns, and tabs.
#[inline]
fn escape_json_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Test that .tar.gz files are hashed as regular files (not introspected) when no colon is present.
    #[test]
    fn test_naive_tar_gz_hashing() {
        // Create a temporary .tar.gz file
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let tar_gz_path = temp_dir.path().join("test.tar.gz");

        // Create some test content in the tar.gz file
        let test_content = b"test tar gz content for hashing";
        fs::write(&tar_gz_path, test_content).expect("Failed to write tar.gz file");

        // Create HashConfig to hash the .tar.gz file as a regular file
        let config = HashConfig {
            input_file: &tar_gz_path,
            recursive: false,
            hash_output: None,
            format: None,
            pretty: false,
            per_file: false,
            no_progress: true,
            include: &[],
            exclude: &[],
            no_ignore: false,
            algo: HashingAlgo::Md5,
            chunk_size_kb: 1024,
            parallel_readers: 1,
            max_files_batch: 1000,
            absolute_paths: false,
        };

        // The hash should succeed and treat the .tar.gz as a regular file
        let result = config.execute_hash();
        assert!(result.is_ok(), "Should hash .tar.gz file as regular file");

        // Verify that no archive introspection occurred by checking no colon was detected
        assert!(
            archive_path::parse_archive_path(&tar_gz_path.to_string_lossy()).is_none(),
            "Should not detect archive path syntax without colon"
        );
    }

    /// Test that .zip files are hashed as regular files (not introspected) when no colon is present.
    #[test]
    fn test_naive_zip_hashing() {
        // Create a temporary .zip file
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let zip_path = temp_dir.path().join("test.zip");

        // Create some test content in the zip file
        let test_content = b"test zip content for hashing";
        fs::write(&zip_path, test_content).expect("Failed to write zip file");

        // Create HashConfig to hash the .zip file as a regular file
        let config = HashConfig {
            input_file: &zip_path,
            recursive: false,
            hash_output: None,
            format: None,
            pretty: false,
            per_file: false,
            no_progress: true,
            include: &[],
            exclude: &[],
            no_ignore: false,
            algo: HashingAlgo::Md5,
            chunk_size_kb: 1024,
            parallel_readers: 1,
            max_files_batch: 1000,
            absolute_paths: false,
        };

        // The hash should succeed and treat the .zip as a regular file
        let result = config.execute_hash();
        assert!(result.is_ok(), "Should hash .zip file as regular file");

        // Verify that no archive introspection occurred
        assert!(
            archive_path::parse_archive_path(&zip_path.to_string_lossy()).is_none(),
            "Should not detect archive path syntax without colon"
        );
    }

    /// Test that colon syntax triggers archive introspection.
    #[test]
    fn test_colon_triggers_introspection() {
        // Test that a path with colon syntax is detected as archive path
        let archive_path_str = "test.tar.gz:internal/file.txt";
        let components = archive_path::parse_archive_path(archive_path_str);
        assert!(
            components.is_some(),
            "Should detect archive path with colon syntax"
        );

        let components = components.expect("Archive components should exist");
        assert_eq!(components.archive().to_string_lossy(), "test.tar.gz");
        assert_eq!(components.entry(), "internal/file.txt");

        // Test ZIP archive path
        let zip_archive_path = "data.zip:output/results.csv";
        let zip_components = archive_path::parse_archive_path(zip_archive_path);
        assert!(
            zip_components.is_some(),
            "Should detect ZIP archive path with colon syntax"
        );

        let zip_components = zip_components.expect("ZIP archive components should exist");
        assert_eq!(zip_components.archive().to_string_lossy(), "data.zip");
        assert_eq!(zip_components.entry(), "output/results.csv");

        // Test that regular paths without colon are not detected as archive paths
        assert!(
            archive_path::parse_archive_path("regular_file.txt").is_none(),
            "Should not detect regular file as archive path"
        );
        assert!(
            archive_path::parse_archive_path("test.tar.gz").is_none(),
            "Should not detect archive file without colon as archive path"
        );
    }
}
