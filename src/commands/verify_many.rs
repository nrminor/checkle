#![allow(clippy::print_stdout)]

use crate::{
    archive_path,
    cli::{NoProgress, OutputFormat, PerFileMode, PrettyPrint},
    data_source::DataSource,
    errors::{CheckleError, Result},
    io::FilesToCheck,
    prelude::*,
    prettyprint::{
        VerificationResult, VerificationStatus, display_verification_table_with_summary,
    },
    progress::ProgressManager,
};
use log::{debug, error, info, warn};
use std::{
    fmt::Write,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

/// Configuration for the verify-many command.
#[derive(Debug, Clone)]
pub struct VerifyManyConfig<'a> {
    pub checksum_file: Option<&'a Path>,
    pub per_file: PerFileMode,
    pub files: &'a [PathBuf],
    pub pretty: PrettyPrint,
    pub report: Option<&'a Path>,
    pub format: Option<OutputFormat>,
    pub algo: HashingAlgo,
    pub chunk_size_kb: usize,
    pub parallel_readers: usize,
    pub max_files_batch: usize,
    pub no_progress: NoProgress,
}

impl VerifyManyConfig<'_> {
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

        // In per-file mode, files must be provided
        if self.per_file && self.files.is_empty() {
            return Err(CheckleError::ConfigError(
                "Files must be provided when using per-file mode".to_string(),
            ));
        }

        // In checksum file mode, checksum file must be provided
        if !self.per_file && self.checksum_file.is_none() {
            return Err(CheckleError::ConfigError(
                "Checksum file must be provided when not using per-file mode".to_string(),
            ));
        }

        Ok(())
    }
}

/// Execute the verify-many command to check multiple files against pre-computed hashes.
///
/// This function supports:
/// - Reading hashes from checksum files (regular or within archives)
/// - Per-file hash verification
/// - Multiple output formats (text, CSV, JSON)
/// - Pretty table display
/// - Report generation
///
/// # Arguments
///
/// * `checksum_file` - Path to checksum file (can be archive path)
/// * `per_file` - Whether to read hashes from per-file hash files
/// * `files` - List of files to verify (only used with `per_file` mode)
/// * `pretty` - Whether to display results in a pretty table
/// * `report` - Optional path to write report file
/// * `format` - Optional output format (auto-detected if not specified)
/// * `algo` - Hashing algorithm to use
/// * `chunk_size_kb` - Chunk size in KB for hashing
/// * `parallel_readers` - Number of parallel readers for hashing
/// * `max_files_batch` - Maximum number of files to process in batch
/// * `no_progress` - Whether to disable progress display
///
/// # Errors
///
/// Returns an error if any files fail verification
// TODO: Refactor the caller (main.rs) to create VerifyManyConfig directly instead of passing all parameters
#[allow(clippy::too_many_arguments)]
pub fn execute(
    checksum_file: Option<&Path>,
    per_file: PerFileMode,
    files: &[PathBuf],
    pretty: PrettyPrint,
    report: Option<&Path>,
    format: Option<OutputFormat>,
    algo: HashingAlgo,
    chunk_size_kb: usize,
    parallel_readers: usize,
    max_files_batch: usize,
    no_progress: NoProgress,
) -> Result<()> {
    let config = VerifyManyConfig {
        checksum_file,
        per_file,
        files,
        pretty,
        report,
        format,
        algo,
        chunk_size_kb,
        parallel_readers,
        max_files_batch,
        no_progress,
    };

    config.execute_verification()
}

impl VerifyManyConfig<'_> {
    /// Execute the verification with this configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration validation fails
    /// - Checksum file cannot be read
    /// - Hash verification fails
    pub fn execute_verification(&self) -> Result<()> {
        // Validate configuration first
        self.validate()?;

        if self.per_file {
            self.verify_per_file_hashes()
        } else {
            self.verify_from_checksum_file()
        }
    }

    /// Verify files using their individual per-file hash files.
    fn verify_per_file_hashes(&self) -> Result<()> {
        // Assert: files must not be empty (validated earlier)
        assert!(
            !self.files.is_empty(),
            "Files must not be empty in per-file mode"
        );

        // Check if the number of files exceeds the batch limit
        if self.files.len() > self.max_files_batch {
            return Err(CheckleError::ExceededFileBatchSize {
                found: self.files.len(),
                limit: self.max_files_batch,
            });
        }

        // Create progress manager for per-file verification
        let show_progress = !self.no_progress;
        let progress_manager = ProgressManager::new(show_progress, self.files.len());

        let mut verification_results = Vec::new();

        for file_path in self.files {
            let verification_result = self.verify_single_per_file(file_path, &progress_manager);
            verification_results.push(verification_result);

            // Update overall progress
            progress_manager.inc_overall();
        }

        // Finish progress display
        progress_manager.finish_with_message(&format!("Verified {} files", self.files.len()));

        // Handle output based on flags
        if self.report.is_some() || self.format.is_some() {
            output_verification_report(
                &verification_results,
                self.report,
                self.format,
                self.pretty,
            )?;
        } else if self.pretty {
            display_verification_results_pretty(&verification_results)?;
        } else {
            output_structured_verification(&verification_results)?;
        }

        // Check if any verifications failed
        check_verification_failures(&verification_results)
    }

    /// Verify a single file using its per-file hash.
    fn verify_single_per_file(
        &self,
        file_path: &Path,
        progress_manager: &ProgressManager,
    ) -> Result<VerificationResult> {
        // Read the expected hash from per-file hash file
        let expected_hash = match read_per_file_hash(file_path, self.algo) {
            Ok(hash) => hash,
            Err(e) => {
                return Ok(VerificationResult::new_error(
                    file_path.to_path_buf(),
                    String::new(),
                    format!("Cannot read hash file: {e}"),
                ));
            }
        };

        // Check if the file exists before trying to compute its hash
        if !file_path.exists() {
            return Ok(VerificationResult::new_missing(
                file_path.to_path_buf(),
                expected_hash,
            ));
        }

        // Get file size for progress tracking
        let file_size = std::fs::metadata(file_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        // Create per-file progress bar if the file is large enough
        let file_progress =
            progress_manager.create_file_progress(file_path.to_string_lossy().as_ref(), file_size);

        // Compute the hash with optional progress callback
        let computed_hash = match file_progress {
            Some(progress) => {
                // With progress callback
                match compute_file_hash_with_progress(
                    file_path,
                    self.algo,
                    self.chunk_size_kb,
                    self.parallel_readers,
                    progress,
                ) {
                    Ok(hash) => hash,
                    Err(e) => {
                        return Ok(VerificationResult::new_error(
                            file_path.to_path_buf(),
                            expected_hash,
                            e.to_string(),
                        ));
                    }
                }
            }
            None => {
                // Without progress callback
                match compute_file_hash(
                    file_path,
                    self.algo,
                    self.chunk_size_kb,
                    self.parallel_readers,
                ) {
                    Ok(hash) => hash,
                    Err(e) => {
                        return Ok(VerificationResult::new_error(
                            file_path.to_path_buf(),
                            expected_hash,
                            e.to_string(),
                        ));
                    }
                }
            }
        };

        let passed = computed_hash == expected_hash;

        // Try to get metadata for enhanced result
        let Ok(meta) = std::fs::metadata(file_path) else {
            return VerificationResult::new(
                file_path.to_path_buf(),
                expected_hash,
                computed_hash,
                passed,
            );
        };

        VerificationResult::new_with_metadata(
            file_path.to_path_buf(),
            expected_hash,
            computed_hash,
            passed,
            &meta,
        )
    }

    /// Verify files from a checksum file.
    fn verify_from_checksum_file(&self) -> Result<()> {
        // Assert: checksum_file must be Some (validated earlier)
        assert!(
            self.checksum_file.is_some(),
            "Checksum file must be provided in checksum file mode"
        );

        let checksum_file_path = self
            .checksum_file
            .expect("checksum_file must be Some (validated)");

        if self.pretty || self.report.is_some() || self.format.is_some() {
            self.verify_with_full_reporting(checksum_file_path)
        } else {
            verify_with_simple_output(checksum_file_path, self.algo, self.max_files_batch)
        }
    }

    /// Verify with full reporting capabilities (pretty/report/format).
    fn verify_with_full_reporting(&self, checksum_file_path: &Path) -> Result<()> {
        // Parse the checksum file including missing files for complete verification reporting
        let file_hash_pairs = parse_checksum_file_raw_with_archive_support(checksum_file_path)?;

        // Check if we've exceeded the max files batch limit
        if file_hash_pairs.len() > self.max_files_batch {
            return Err(CheckleError::ExceededFileBatchSize {
                found: file_hash_pairs.len(),
                limit: self.max_files_batch,
            });
        }

        // Create progress manager for checksum file verification
        let show_progress = !self.no_progress;
        let progress_manager = ProgressManager::new(show_progress, file_hash_pairs.len());

        let mut verification_results = Vec::new();

        let file_count = file_hash_pairs.len();
        for (file_path, expected_hash) in file_hash_pairs {
            let verification_result = verify_single_file(
                &file_path,
                &expected_hash,
                self.algo,
                self.chunk_size_kb,
                self.parallel_readers,
                &progress_manager,
            );
            verification_results.push(verification_result);

            // Update overall progress
            progress_manager.inc_overall();
        }

        // Finish progress display
        progress_manager.finish_with_message(&format!("Verified {file_count} files"));

        // Handle output based on flags
        if self.report.is_some() || self.format.is_some() {
            output_verification_report(
                &verification_results,
                self.report,
                self.format,
                self.pretty,
            )?;
        } else if self.pretty {
            display_verification_results_pretty(&verification_results)?;
        }

        // Check if any verifications failed
        check_verification_failures(&verification_results)
    }
}

/// Verify a single file from checksum file entry with progress support.
fn verify_single_file(
    file_path: &Path,
    expected_hash: &str,
    algo: HashingAlgo,
    chunk_size_kb: usize,
    parallel_readers: usize,
    progress_manager: &ProgressManager,
) -> Result<VerificationResult> {
    // Try to create a DataSource for this file path (supports both filesystem and archive paths)
    let Ok(source) = create_data_source_from_path(file_path) else {
        // DataSource creation failed - file/archive entry doesn't exist
        return Ok(VerificationResult::new_missing(
            file_path.to_path_buf(),
            expected_hash.to_string(),
        ));
    };

    // Get file size for progress tracking
    let file_size = source
        .as_path()
        .and_then(|fs_path| std::fs::metadata(fs_path).ok())
        .map_or(0, |metadata| metadata.len());

    // Create per-file progress bar if the file is large enough
    let file_progress =
        progress_manager.create_file_progress(file_path.to_string_lossy().as_ref(), file_size);

    let chunk_size_kb_u16 = convert_chunk_size_kb(chunk_size_kb);

    // Compute the hash with optional progress callback
    let computed_hash = match file_progress {
        Some(progress) => {
            // With progress callback
            match source.hash(
                algo,
                chunk_size_kb_u16,
                parallel_readers,
                Some(move |bytes_read| {
                    progress.update(bytes_read);
                }),
            ) {
                Ok(hash) => hash,
                Err(e) => {
                    return Ok(VerificationResult::new_error(
                        file_path.to_path_buf(),
                        expected_hash.to_string(),
                        e.to_string(),
                    ));
                }
            }
        }
        None => {
            // Without progress callback
            match source.hash(algo, chunk_size_kb_u16, parallel_readers, None::<fn(u64)>) {
                Ok(hash) => hash,
                Err(e) => {
                    return Ok(VerificationResult::new_error(
                        file_path.to_path_buf(),
                        expected_hash.to_string(),
                        e.to_string(),
                    ));
                }
            }
        }
    };

    let passed = computed_hash == expected_hash;

    // Try to get metadata if it's a filesystem path
    let metadata = source
        .as_path()
        .and_then(|fs_path| std::fs::metadata(fs_path).ok());

    match metadata {
        Some(meta) => VerificationResult::new_with_metadata(
            file_path.to_path_buf(),
            expected_hash.to_string(),
            computed_hash,
            passed,
            &meta,
        ),
        None => VerificationResult::new(
            file_path.to_path_buf(),
            expected_hash.to_string(),
            computed_hash,
            passed,
        ),
    }
}

/// Verify with simple output (using `FilesToCheck`).
fn verify_with_simple_output(
    checksum_file_path: &Path,
    algo: HashingAlgo,
    max_files_batch: usize,
) -> Result<()> {
    let checksum_file_str = checksum_file_path.to_string_lossy();

    let files_to_check =
        if let Some(archive_components) = archive_path::parse_archive_path(&checksum_file_str) {
            // Checksum file is within an archive
            debug!(
                "Processing checksum file from archive: {}:{}",
                archive_components.archive().display(),
                archive_components.entry()
            );
            // Check if archive exists before trying to read from it
            if !archive_components.archive().exists() {
                return Err(CheckleError::InaccessibleFile(
                    archive_components.archive().to_path_buf(),
                ));
            }
            FilesToCheck::new_from_archive(&archive_components)?
        } else {
            // Regular checksum file
            FilesToCheck::new_from_txt(checksum_file_path)?
        };

    // Check if the number of files exceeds the batch limit
    let file_count = files_to_check.len();
    if file_count > max_files_batch {
        return Err(CheckleError::ExceededFileBatchSize {
            found: file_count,
            limit: max_files_batch,
        });
    }

    files_to_check.checksum_all(&algo)?;
    Ok(())
}

/// Output verification report in the requested format.
fn output_verification_report(
    verification_results: &[Result<VerificationResult>],
    report: Option<&Path>,
    format: Option<OutputFormat>,
    pretty: PrettyPrint,
) -> Result<()> {
    // Detect format from file extension if not explicitly provided
    let output_format = if let Some(fmt) = format {
        fmt
    } else if let Some(report_path) = report {
        OutputFormat::detect_from_path(report_path)
    } else {
        OutputFormat::Text
    };

    // Collect successful verification results and handle errors
    let mut successful_results: Vec<VerificationResult> = Vec::new();
    let mut error_count = 0;

    for result in verification_results {
        match result {
            Ok(verification_result) => {
                successful_results.push(verification_result.clone());
            }
            Err(e) => {
                error!("Error during verification: {e}");
                error_count += 1;
            }
        }
    }

    // If we have errors but also successful results, log and continue
    if error_count > 0 && !successful_results.is_empty() {
        warn!(
            "{} verification errors occurred, reporting {} successful verifications",
            error_count,
            successful_results.len()
        );
    } else if error_count > 0 {
        // All verifications failed - return a generic error
        return Err(CheckleError::MultipleFailedChecksums);
    }

    let formatted_output =
        format_verification_output_with_pretty(&successful_results, output_format, pretty);

    if let Some(report_path) = report {
        // Write to file
        std::fs::write(report_path, formatted_output).map_err(|e| CheckleError::FileOpenError {
            path: report_path.to_path_buf(),
            source: e,
        })?;

        info!("Verification report written to: {}", report_path.display());
        // Do NOT print to stdout when report file is specified
    } else if !pretty {
        // Write to stdout only when no report file and not in pretty mode
        println!("{formatted_output}");
    }
    // Note: pretty mode displays to stderr via display_verification_table_with_summary

    Ok(())
}

/// Display verification results in a pretty table.
fn display_verification_results_pretty(
    verification_results: &[Result<VerificationResult>],
) -> Result<()> {
    let mut display_results: Vec<VerificationResult> = Vec::new();

    for result in verification_results {
        match result {
            Ok(verification_result) => {
                display_results.push(verification_result.clone());
            }
            Err(e) => {
                error!("Error during verification: {e}");
            }
        }
    }

    display_verification_table_with_summary(&display_results)?;
    Ok(())
}

/// Output verification results in structured format (tab-delimited).
#[allow(clippy::print_stdout)]
fn output_structured_verification(
    verification_results: &[Result<VerificationResult>],
) -> Result<()> {
    // Collect all output lines first to print as a single block (avoids interleaving with logs)
    let mut output_lines = Vec::with_capacity(verification_results.len());
    let mut failed_files = Vec::with_capacity(verification_results.len());
    let mut successful_count = 0;

    for result in verification_results {
        match result {
            Ok(verification_result) => match verification_result.status() {
                VerificationStatus::Pass => {
                    output_lines.push(format!("PASS\t{}", verification_result.file().display()));
                    info!("Verified: {}", verification_result.file().display());
                    successful_count += 1;
                }
                VerificationStatus::Fail => {
                    output_lines.push(format!("FAIL\t{}", verification_result.file().display()));
                    error!(
                        "Verification failed for {}: {}",
                        verification_result.file().display(),
                        verification_result
                            .error_message()
                            .unwrap_or("Hash mismatch")
                    );
                    failed_files.push(verification_result.file().to_path_buf());
                }
                VerificationStatus::Missing => {
                    output_lines.push(format!("MISS\t{}", verification_result.file().display()));
                    error!("File not found: {}", verification_result.file().display());
                    failed_files.push(verification_result.file().to_path_buf());
                }
                VerificationStatus::Error(_) => {
                    output_lines.push(format!("ERROR\t{}", verification_result.file().display()));
                    error!(
                        "Error verifying {}: {}",
                        verification_result.file().display(),
                        verification_result
                            .error_message()
                            .unwrap_or("Unknown error")
                    );
                    failed_files.push(verification_result.file().to_path_buf());
                }
            },
            Err(e) => {
                // Handle verification errors - these are errors that occurred during the verification process
                error!("Verification error: {e}");
                // We don't have a file path for these errors since the verification itself failed
            }
        }
    }

    // Print all verification results as a single block to stdout
    if !output_lines.is_empty() {
        println!("{}", output_lines.join("\n"));
    }

    info!("Verified {successful_count} files successfully");

    if !failed_files.is_empty() {
        error!("{} files failed verification", failed_files.len());
        return Err(CheckleError::MultipleFailedChecksums);
    }

    Ok(())
}

/// Check if any verifications failed and return appropriate error.
fn check_verification_failures(verification_results: &[Result<VerificationResult>]) -> Result<()> {
    let has_failures = verification_results.iter().any(|r| {
        match r {
            Ok(verification_result) => !verification_result.passed(),
            Err(_) => true, // Errors are also considered failures
        }
    });

    if has_failures {
        Err(CheckleError::MultipleFailedChecksums)
    } else {
        Ok(())
    }
}

// Helper functions

/// Compute hash for a file.
fn compute_file_hash(
    file_path: &Path,
    algo: HashingAlgo,
    chunk_size_kb: usize,
    parallel_readers: usize,
) -> Result<String> {
    match algo {
        HashingAlgo::Md5 => {
            let hasher = Hasher::new_md5(file_path);
            let configured_hasher = configure_hasher(hasher, chunk_size_kb, parallel_readers)?;
            configured_hasher.find_root_hash()
        }
        HashingAlgo::Sha2 => {
            let hasher = Hasher::new_sha2(file_path);
            let configured_hasher = configure_hasher(hasher, chunk_size_kb, parallel_readers)?;
            configured_hasher.find_root_hash()
        }
    }
}

/// Compute hash for a file with progress callback.
fn compute_file_hash_with_progress(
    file_path: &Path,
    algo: HashingAlgo,
    chunk_size_kb: usize,
    parallel_readers: usize,
    progress: crate::progress::FileProgress,
) -> Result<String> {
    match algo {
        HashingAlgo::Md5 => {
            let hasher = Hasher::new_md5(file_path);
            let configured_hasher = configure_hasher(hasher, chunk_size_kb, parallel_readers)?;

            // Add progress callback
            let configured_hasher =
                configured_hasher.with_progress_callback(Box::new(move |bytes_read| {
                    progress.update(bytes_read);
                }));

            configured_hasher.find_root_hash()
        }
        HashingAlgo::Sha2 => {
            let hasher = Hasher::new_sha2(file_path);
            let configured_hasher = configure_hasher(hasher, chunk_size_kb, parallel_readers)?;

            // Add progress callback
            let configured_hasher =
                configured_hasher.with_progress_callback(Box::new(move |bytes_read| {
                    progress.update(bytes_read);
                }));

            configured_hasher.find_root_hash()
        }
    }
}

/// Configure hasher with options.
fn configure_hasher<const N: usize>(
    hasher: Hasher<'_, N>,
    chunk_size_kb: usize,
    parallel_readers: usize,
) -> Result<Hasher<'_, N>> {
    let mut configured_hasher = hasher;

    // Configure chunk size if different from default
    let chunk_size_bytes = chunk_size_kb * 1024;
    configured_hasher = configured_hasher.with_chunk_size(chunk_size_bytes)?;

    // Configure parallel readers (0 = auto-detect)
    if parallel_readers > 0 {
        configured_hasher = configured_hasher.with_parallel_readers(parallel_readers);
    }

    Ok(configured_hasher)
}

/// Read hash from per-file hash file.
///
/// Reads the hash from a file with .md5 or .sha256 extension.
/// Supports both hash-only format and "hash filename" format.
#[inline]
fn read_per_file_hash(file_path: &Path, algorithm: HashingAlgo) -> Result<String> {
    let hash_file_path = get_per_file_hash_path(file_path, algorithm);

    if !hash_file_path.exists() {
        return Err(CheckleError::InaccessibleFile(hash_file_path));
    }

    let content = fs::read_to_string(&hash_file_path).map_err(|e| CheckleError::FileReadError {
        path: hash_file_path.clone(),
        source: e,
    })?;

    // Extract the hash from the first line
    let first_line = content
        .lines()
        .next()
        .ok_or_else(|| CheckleError::InvalidChecksumFile(hash_file_path.clone()))?
        .trim();

    // If the line contains whitespace, assume it's in "hash filename" format
    let hash = if first_line.contains(char::is_whitespace) {
        first_line
            .split_whitespace()
            .next()
            .ok_or_else(|| CheckleError::InvalidChecksumFile(hash_file_path.clone()))?
            .to_string()
    } else {
        first_line.to_string()
    };

    Ok(hash)
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

/// Parse checksum file including missing files, supporting archive paths.
fn parse_checksum_file_raw_with_archive_support(
    checksum_file_path: &Path,
) -> Result<Vec<(PathBuf, String)>> {
    let checksum_file_str = checksum_file_path.to_string_lossy();

    if let Some(archive_components) = archive_path::parse_archive_path(&checksum_file_str) {
        // Check if archive exists before trying to read from it
        if !archive_components.archive().exists() {
            return Err(CheckleError::InaccessibleFile(
                archive_components.archive().to_path_buf(),
            ));
        }
        // Checksum file is within an archive - use archive reading logic
        let checksum_content = crate::io::read_file_from_archive(&archive_components)?;
        let mut pairs = Vec::new();

        for line in checksum_content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

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

            // Skip if this file is the checksum file itself
            // For archive paths, we can't easily compare, so we don't skip
            pairs.push((PathBuf::from(file_str), hash.to_string()));
        }

        Ok(pairs)
    } else {
        // Regular file - use existing logic
        parse_checksum_file_raw(checksum_file_path)
    }
}

/// Parse checksum file including missing files.
fn parse_checksum_file_raw(checksum_file: &Path) -> Result<Vec<(PathBuf, String)>> {
    let file_handle = File::open(checksum_file)
        .map_err(|_| CheckleError::InaccessibleFile(checksum_file.to_path_buf()))?;
    let buffer = BufReader::new(file_handle);

    // Get the canonical path of the checksum file for comparison
    let checksum_file_canonical = checksum_file
        .canonicalize()
        .unwrap_or_else(|_| checksum_file.to_path_buf());

    let mut pairs = Vec::new();

    for line in buffer.lines() {
        let line =
            line.map_err(|_| CheckleError::InvalidChecksumFile(checksum_file.to_path_buf()))?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

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

        // Skip if this file is the checksum file itself
        // Try to canonicalize the file path for comparison, but if it fails (file doesn't exist yet),
        // just compare the paths as-is
        let file_canonical = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.clone());
        if file_canonical == checksum_file_canonical {
            debug!("Skipping checksum file itself: {file_str}");
            continue;
        }

        pairs.push((file_path, hash.to_string()));
    }

    Ok(pairs)
}

/// Create a `DataSource` from a path that might contain archive syntax.
///
/// Handles both regular filesystem paths and archive paths (e.g., archive.tar:entry).
#[inline]
fn create_data_source_from_path(file_path: &Path) -> Result<crate::data_source::DataSource> {
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

/// Format verification output with optional pretty printing.
fn format_verification_output_with_pretty(
    results: &[VerificationResult],
    format: OutputFormat,
    pretty: PrettyPrint,
) -> String {
    match format {
        OutputFormat::Text => format_text_output(results),
        OutputFormat::Csv => format_csv_output(results),
        OutputFormat::Json => format_json_output_with_pretty(results, pretty),
    }
}

fn format_text_output(results: &[VerificationResult]) -> String {
    let mut output = String::from(
        "file_path\tstatus\texpected_hash\tcomputed_hash\tfile_size_bytes\tmodified_time\terror_message\n",
    );

    for result in results {
        let file_path = result.file().to_string_lossy();
        let status = result.status().display_string();
        let expected_hash = result.expected_hash();
        let computed_hash = if result.actual_hash().is_empty() {
            ""
        } else {
            result.actual_hash()
        };
        let file_size = result
            .file_size()
            .map_or_else(String::new, |s| s.to_string());
        let modified_time = result
            .modified_time()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(String::new, |d| d.as_secs().to_string());
        let error_message = result.error_message().unwrap_or("");

        writeln!(
            output,
            "{file_path}\t{status}\t{expected_hash}\t{computed_hash}\t{file_size}\t{modified_time}\t{error_message}"
        )
        .expect("Writing to String should never fail");
    }

    output.trim_end().to_string()
}

fn format_csv_output(results: &[VerificationResult]) -> String {
    let mut output = String::from(
        "file_path,status,expected_hash,computed_hash,file_size_bytes,modified_time,error_message\n",
    );

    for result in results {
        let file_path = result.file().to_string_lossy();
        let status = result.status().display_string();
        let expected_hash = result.expected_hash();
        let computed_hash = if result.actual_hash().is_empty() {
            ""
        } else {
            result.actual_hash()
        };
        let file_size = result
            .file_size()
            .map_or_else(String::new, |s| s.to_string());
        let modified_time = result
            .modified_time()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(String::new, |d| d.as_secs().to_string());
        let error_message = result.error_message().unwrap_or("");

        // Escape CSV fields
        let escaped_file_path = if file_path.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", file_path.replace('"', "\"\""))
        } else {
            file_path.to_string()
        };

        let escaped_error = if error_message.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", error_message.replace('"', "\"\""))
        } else {
            error_message.to_string()
        };

        writeln!(
            output,
            "{escaped_file_path},{status},{expected_hash},{computed_hash},{file_size},{modified_time},{escaped_error}"
        )
        .expect("Writing to String should never fail");
    }

    output.trim_end().to_string()
}

fn format_json_output_with_pretty(results: &[VerificationResult], pretty: bool) -> String {
    if pretty {
        format_json_output_pretty(results)
    } else {
        format_json_output_compact(results)
    }
}

fn format_json_output_compact(results: &[VerificationResult]) -> String {
    let mut json_objects = Vec::new();

    for result in results {
        let file_path = result.file().to_string_lossy();
        let status = result.status().display_string();
        let expected_hash = result.expected_hash();
        let computed_hash = if result.actual_hash().is_empty() {
            "null".to_string()
        } else {
            format!("\"{}\"", result.actual_hash())
        };
        let file_size = result
            .file_size()
            .map_or_else(|| "null".to_string(), |s| s.to_string());
        let modified_time = result
            .modified_time()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(|| "null".to_string(), |d| d.as_secs().to_string());
        let error_message = result.error_message().map_or_else(
            || "null".to_string(),
            |e| format!("\"{}\"", e.replace('\\', "\\\\").replace('"', "\\\"")),
        );

        // Escape JSON strings
        let escaped_file_path = escape_json_string(&file_path);

        json_objects.push(format!(
            "{{\"file_path\":\"{escaped_file_path}\",\"status\":\"{status}\",\"expected_hash\":\"{expected_hash}\",\"computed_hash\":{computed_hash},\"file_size_bytes\":{file_size},\"modified_time\":{modified_time},\"error_message\":{error_message}}}"
        ));
    }

    format!("[{}]", json_objects.join(","))
}

fn format_json_output_pretty(results: &[VerificationResult]) -> String {
    let mut output = String::from("[\n");
    for (i, result) in results.iter().enumerate() {
        let file_path = result.file().to_string_lossy();
        let status = result.status().display_string();
        let expected_hash = result.expected_hash();
        let computed_hash = if result.actual_hash().is_empty() {
            "null".to_string()
        } else {
            format!("\"{}\"", result.actual_hash())
        };
        let file_size = result
            .file_size()
            .map_or_else(|| "null".to_string(), |s| s.to_string());
        let modified_time = result
            .modified_time()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(|| "null".to_string(), |d| d.as_secs().to_string());
        let error_message = result.error_message().map_or_else(
            || "null".to_string(),
            |e| format!("\"{}\"", e.replace('\\', "\\\\").replace('"', "\\\"")),
        );

        // Escape JSON strings
        let escaped_file_path = escape_json_string(&file_path);

        output.push_str("  {\n");
        writeln!(output, "    \"file_path\": \"{escaped_file_path}\",")
            .expect("Failed to write file_path to output");
        writeln!(output, "    \"status\": \"{status}\",")
            .expect("Failed to write status to output");
        writeln!(output, "    \"expected_hash\": \"{expected_hash}\",")
            .expect("Failed to write expected_hash to output");
        writeln!(output, "    \"computed_hash\": {computed_hash},")
            .expect("Failed to write computed_hash to output");
        writeln!(output, "    \"file_size_bytes\": {file_size},")
            .expect("Failed to write file_size_bytes to output");
        writeln!(output, "    \"modified_time\": {modified_time},")
            .expect("Failed to write modified_time to output");
        writeln!(output, "    \"error_message\": {error_message}")
            .expect("Failed to write error_message to output");

        if i == results.len() - 1 {
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
