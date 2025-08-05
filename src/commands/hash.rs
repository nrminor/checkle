#![allow(clippy::print_stdout)]

#[cfg(feature = "tar")]
use crate::archive::TarArchive;
#[cfg(feature = "zip")]
use crate::archive::ZipArchive;
use crate::{
    archive_path,
    cli::{NoIgnore, NoProgress, OutputFormat, PerFileMode, PrettyPrint, Recursive},
    errors::{CheckleError, Result},
    io::{FileFilterConfig, FileHashPair, collect_files},
    prelude::*,
    prettyprint::{FileHashPairWithMetadata, convert_to_basic_pairs, display_pretty_table},
    progress::ProgressManager,
};
use log::{debug, error, info, warn};
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

        // Check for archive traversal: --recursive with an archive file (not archive path)
        let input_file_str = self.input_file.to_string_lossy();
        if self.recursive && archive_path::parse_archive_path(&input_file_str).is_none() {
            // Check if input_file is an archive file itself (not an archive path)
            if is_archive_file(self.input_file) {
                // Archive traversal - hash all entries in the archive
                return self.hash_archive_entries();
            }
        }

        // Check if input_file is an archive path - if so, handle it as a single entry hash
        if let Some(_archive_components) = archive_path::parse_archive_path(&input_file_str) {
            return self.hash_single_archive_entry();
        }

        // Regular file/directory hashing
        self.hash_regular_files(&filter_config)
    }

    /// Hash a single archive entry using archive path syntax.
    fn hash_single_archive_entry(&self) -> Result<()> {
        // Archive path - hash single entry using DataSource
        let source = create_data_source_from_path(self.input_file)?;
        let chunk_size_kb = convert_chunk_size_kb(self.chunk_size_kb);

        let hash = source.hash(
            self.algo,
            chunk_size_kb,
            self.parallel_readers,
            None::<fn(u64)>,
        )?;

        // Create result with archive metadata if possible, otherwise use fallback
        let result = if let Some(fs_path) = source.as_path() {
            if let Ok(metadata) = std::fs::metadata(fs_path) {
                FileHashPairWithMetadata::new(
                    self.input_file.to_path_buf(),
                    hash.clone(),
                    &metadata,
                )?
            } else {
                FileHashPairWithMetadata::new_with_fallback(
                    self.input_file.to_path_buf(),
                    hash.clone(),
                )?
            }
        } else {
            // Archive source - use fallback (no filesystem metadata)
            FileHashPairWithMetadata::new_with_fallback(
                self.input_file.to_path_buf(),
                hash.clone(),
            )?
        };

        let file_hash_pairs = vec![result];

        // Handle output
        if self.per_file {
            // Write per-file hash for the single archive entry
            write_per_file_hash(self.input_file, &hash, self.algo)?;
        } else if let Some(output_path) = self.hash_output {
            let output_format = self.format.unwrap_or(OutputFormat::Text);
            let formatted_output = format_output_with_pretty(
                &convert_to_basic_pairs(file_hash_pairs.clone()),
                output_format,
                self.pretty,
            );
            std::fs::write(output_path, formatted_output).map_err(|e| {
                CheckleError::FileOpenError {
                    path: output_path.to_path_buf(),
                    source: e,
                }
            })?;
        } else if self.pretty {
            display_pretty_table(&file_hash_pairs)?;
        } else {
            let output_format = self.format.unwrap_or(OutputFormat::Text);
            let formatted_output = format_output_with_pretty(
                &convert_to_basic_pairs(file_hash_pairs),
                output_format,
                false,
            );
            println!("{formatted_output}");
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
            self.algo,
        )
    }

    /// Hash all entries in an archive file.
    fn hash_archive_entries(&self) -> Result<()> {
        hash_archive_entries(
            self.input_file,
            self.chunk_size_kb,
            self.parallel_readers,
            self.algo,
            self.per_file,
            self.hash_output,
            self.format,
            self.pretty,
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
                format_output_with_pretty(file_hash_pairs, output_format, pretty)
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
        let formatted_output = format_output_with_pretty(file_hash_pairs, output_format, pretty);
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
        let formatted_output = format_output_with_pretty(file_hash_pairs, output_format, pretty);
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

/// Check if a file path represents an archive file based on extension.
#[inline]
fn is_archive_file(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();

    #[cfg(feature = "tar")]
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
        || path_str.contains(".tar.")
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tgz"))
    {
        return true;
    }

    #[cfg(feature = "zip")]
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return true;
    }

    false
}

/// Configuration for hashing archive entries.
struct ArchiveHashConfig<'a> {
    archive_path: &'a Path,
    chunk_size_kb: usize,
    parallel_readers: usize,
    algo: HashingAlgo,
    per_file: PerFileMode,
    hash_output: Option<&'a Path>,
    format: Option<OutputFormat>,
    pretty: PrettyPrint,
}

/// Hash all entries in an archive file.
// TODO: Remove this function once all callers use ArchiveHashConfig directly
#[allow(clippy::too_many_arguments)]
fn hash_archive_entries(
    archive_path: &Path,
    chunk_size_kb: usize,
    parallel_readers: usize,
    algo: HashingAlgo,
    per_file: PerFileMode,
    hash_output: Option<&Path>,
    format: Option<OutputFormat>,
    pretty: PrettyPrint,
) -> Result<()> {
    let config = ArchiveHashConfig {
        archive_path,
        chunk_size_kb,
        parallel_readers,
        algo,
        per_file,
        hash_output,
        format,
        pretty,
    };
    config.hash_entries()
}

impl ArchiveHashConfig<'_> {
    fn hash_entries(&self) -> Result<()> {
        // Determine archive type and create reader
        let archive_path_str = self.archive_path.to_string_lossy().to_lowercase();

        #[cfg(feature = "tar")]
        if self
            .archive_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
            || archive_path_str.contains(".tar.")
            || self
                .archive_path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("tgz"))
        {
            return self.hash_tar_entries();
        }

        #[cfg(feature = "zip")]
        if self
            .archive_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
        {
            return self.hash_zip_entries();
        }

        Err(CheckleError::UnsupportedArchiveFormat(
            self.archive_path.to_path_buf(),
        ))
    }

    #[cfg(feature = "tar")]
    fn hash_tar_entries(&self) -> Result<()> {
        let mut archive = TarArchive::open(self.archive_path)?;
        let entry_count = archive.entry_count()?;

        // Create progress manager
        let show_progress = true; // Always show progress for archive traversal
        let progress_manager = ProgressManager::new(show_progress, entry_count);

        // Get all archive entries first
        let entries = archive.list_entries()?;
        let mut archive_entries = Vec::new();

        // Collect all entry paths
        for entry_path in entries {
            // Create archive path syntax: archive.tar:internal/path
            let full_path = format!("{}:{}", self.archive_path.display(), entry_path);
            archive_entries.push(PathBuf::from(full_path));
        }

        // Now process all entries in parallel using DataSource
        hash_archive_entry_paths(
            archive_entries,
            self.chunk_size_kb,
            self.parallel_readers,
            self.algo,
            self.per_file,
            self.hash_output,
            self.format,
            self.pretty,
            &progress_manager,
        )
    }

    #[cfg(feature = "zip")]
    fn hash_zip_entries(&self) -> Result<()> {
        let mut archive = ZipArchive::open(self.archive_path)?;
        let entry_count = archive.entry_count();

        // Create progress manager
        let show_progress = true; // Always show progress for archive traversal
        let progress_manager = ProgressManager::new(show_progress, entry_count);

        // Get all archive entries first
        let entries = archive.list_entries()?;
        let mut archive_entries = Vec::new();

        // Collect all entry paths
        for entry_path in entries {
            // Create archive path syntax: archive.zip:internal/path
            let full_path = format!("{}:{}", self.archive_path.display(), entry_path);
            archive_entries.push(PathBuf::from(full_path));
        }

        // Now process all entries in parallel using DataSource
        hash_archive_entry_paths(
            archive_entries,
            self.chunk_size_kb,
            self.parallel_readers,
            self.algo,
            self.per_file,
            self.hash_output,
            self.format,
            self.pretty,
            &progress_manager,
        )
    }
}

struct ArchiveEntryHashConfig<'a> {
    archive_entries: Vec<PathBuf>,
    chunk_size_kb: usize,
    parallel_readers: usize,
    algo: HashingAlgo,
    per_file: bool,
    hash_output: Option<&'a Path>,
    format: Option<OutputFormat>,
    pretty: bool,
    progress_manager: &'a ProgressManager,
}

// TODO: Remove this function once all callers use ArchiveEntryHashConfig directly
#[allow(clippy::too_many_arguments)]
fn hash_archive_entry_paths(
    archive_entries: Vec<PathBuf>,
    chunk_size_kb: usize,
    parallel_readers: usize,
    algo: HashingAlgo,
    per_file: PerFileMode,
    hash_output: Option<&Path>,
    format: Option<OutputFormat>,
    pretty: PrettyPrint,
    progress_manager: &ProgressManager,
) -> Result<()> {
    let config = ArchiveEntryHashConfig {
        archive_entries,
        chunk_size_kb,
        parallel_readers,
        algo,
        per_file,
        hash_output,
        format,
        pretty,
        progress_manager,
    };
    config.hash_entries()
}

impl ArchiveEntryHashConfig<'_> {
    #[allow(clippy::print_stdout)]
    fn hash_entries(self) -> Result<()> {
        // Clone necessary values for use in parallel iterator
        let progress_manager_clone = self.progress_manager.clone();
        let chunk_size_kb = self.chunk_size_kb;
        let parallel_readers = self.parallel_readers;
        let algo_clone = self.algo;

        let file_hash_pairs = self
            .archive_entries
            .into_par_iter()
            .map(move |entry_path| -> Result<FileHashPairWithMetadata> {
                // Create DataSource for this archive entry
                let source = create_data_source_from_path(&entry_path)?;

                // Extract chunk_size and parallel_readers as we did before
                let chunk_size_kb_u16 = if chunk_size_kb == 0 {
                    0
                } else {
                    u16::try_from(chunk_size_kb).unwrap_or(1024) // Default to 1024 if too large
                };

                let hash = source.hash(
                    algo_clone,
                    chunk_size_kb_u16,
                    parallel_readers,
                    None::<fn(u64)>,
                )?;

                // For archive entries, we use fallback metadata since there's no filesystem file
                let result = FileHashPairWithMetadata::new_with_fallback(entry_path, hash)?;

                // Update progress
                progress_manager_clone.inc_overall();

                Ok(result)
            })
            .collect::<Vec<_>>();

        // Handle results and errors
        let mut successful_results = Vec::new();
        let mut error_count = 0;

        for result in file_hash_pairs {
            match result {
                Ok(file_hash_pair) => {
                    successful_results.push(file_hash_pair);
                }
                Err(e) => {
                    error!("Error hashing archive entry: {e}");
                    error_count += 1;
                }
            }
        }

        if successful_results.is_empty() {
            return Err(CheckleError::MultipleFailedChecksums);
        }

        if error_count > 0 {
            warn!("Failed to hash {error_count} archive entries");
        }

        // Handle output (same as regular hash command)
        if self.per_file {
            for result in &successful_results {
                write_per_file_hash(result.file(), result.hash(), self.algo)?;
            }
        } else if let Some(output_path) = self.hash_output {
            let output_format = self.format.unwrap_or(OutputFormat::Text);
            let formatted_output = format_output_with_pretty(
                &convert_to_basic_pairs(successful_results),
                output_format,
                self.pretty,
            );
            std::fs::write(output_path, formatted_output).map_err(|e| {
                CheckleError::FileOpenError {
                    path: output_path.to_path_buf(),
                    source: e,
                }
            })?;
        } else if self.pretty {
            display_pretty_table(&successful_results)?;
        } else {
            let output_format = self.format.unwrap_or(OutputFormat::Text);
            let formatted_output = format_output_with_pretty(
                &convert_to_basic_pairs(successful_results),
                output_format,
                false,
            );
            println!("{formatted_output}");
        }

        Ok(())
    }
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
) -> String {
    match format {
        OutputFormat::Text => {
            // Tab-delimited format: hash\tfile_path
            let lines: Vec<String> = file_hash_pairs
                .iter()
                .map(|file| format!("{}\t{}", file.hash(), file.file().to_string_lossy()))
                .collect();
            lines.join("\n")
        }
        OutputFormat::Csv => {
            // CSV format with proper escaping
            let mut output = String::from("hash,filepath\n"); // Header
            for file in file_hash_pairs {
                let hash = file.hash();
                let filepath = file.file().to_string_lossy();

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
                format_hash_json_pretty(file_hash_pairs)
            } else {
                format_hash_json_compact(file_hash_pairs)
            }
        }
    }
}

fn format_hash_json_compact(file_hash_pairs: &[FileHashPair]) -> String {
    let mut json_objects = Vec::new();
    for file in file_hash_pairs {
        let hash = file.hash();
        let filepath = file.file().to_string_lossy();
        let escaped_filepath = escape_json_string(&filepath);
        json_objects.push(format!(
            "{{\"hash\":\"{hash}\",\"filepath\":\"{escaped_filepath}\"}}"
        ));
    }
    format!("[{}]", json_objects.join(","))
}

fn format_hash_json_pretty(file_hash_pairs: &[FileHashPair]) -> String {
    let mut output = String::from("[\n");
    for (i, file) in file_hash_pairs.iter().enumerate() {
        let hash = file.hash();
        let filepath = file.file().to_string_lossy();
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
