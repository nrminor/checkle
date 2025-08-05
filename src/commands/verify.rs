use crate::{
    archive_path,
    cli::{NoProgress, PerFileMode, PrettyPrint},
    data_source::DataSource,
    errors::{CheckleError, Result},
    prelude::*,
    prettyprint::{VerificationResult, display_verification_table},
    progress::ProgressManager,
};
use std::{
    fs,
    path::{Path, PathBuf},
    slice,
};

/// Execute the verify command to check a single file against a known hash.
///
/// This function handles both regular files and archive entries, with support
/// for pretty printing and per-file hash reading.
///
/// # Arguments
///
/// * `input_file` - Path to the file to verify (can be archive path)
/// * `hash` - Optional hash value (if None, reads from per-file hash)
/// * `algo` - Hashing algorithm to use
/// * `per_file` - Whether to read hash from per-file hash file
/// * `pretty` - Whether to display results in a pretty table
/// * `no_progress` - Whether to disable progress display
/// * `chunk_size_kb` - Chunk size in KB for hashing
/// * `parallel_readers` - Number of parallel readers for hashing
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be accessed
/// - Hash computation fails
/// - Verification fails
#[allow(clippy::too_many_arguments)]
pub fn execute(
    input_file: &Path,
    hash: Option<&str>,
    algo: HashingAlgo,
    per_file: PerFileMode,
    pretty: PrettyPrint,
    no_progress: NoProgress,
    chunk_size_kb: usize,
    parallel_readers: usize,
) -> Result<()> {
    let hash_to_verify = if per_file {
        // Read hash from per-file hash file
        read_per_file_hash(input_file, algo)?
    } else {
        // Use provided hash
        hash.ok_or_else(|| CheckleError::InvalidChecksumFile(PathBuf::from("command line")))?
            .to_string()
    };

    if pretty {
        verify_with_pretty_output(
            input_file,
            &hash_to_verify,
            algo,
            no_progress,
            chunk_size_kb,
            parallel_readers,
        )
    } else {
        verify_simple(
            input_file,
            &hash_to_verify,
            algo,
            no_progress,
            chunk_size_kb,
            parallel_readers,
        )
    }
}

/// Verify a file with pretty table output to stderr.
fn verify_with_pretty_output(
    input_file: &Path,
    hash_to_verify: &str,
    algo: HashingAlgo,
    no_progress: NoProgress,
    chunk_size_kb: usize,
    parallel_readers: usize,
) -> Result<()> {
    // Try to create a DataSource for this file path (supports both filesystem and archive paths)
    let source = match create_data_source_from_path(input_file) {
        Ok(source) => source,
        Err(_e) => {
            let result = VerificationResult::new_missing(
                input_file.to_path_buf(),
                hash_to_verify.to_string(),
            );
            display_verification_table(&[result])?;
            return Err(CheckleError::InaccessibleFile(input_file.to_path_buf()));
        }
    };

    // Create progress manager (single file verification)
    let show_progress = !no_progress;
    let progress_manager = ProgressManager::new(show_progress, 1);

    // Get file size for progress tracking
    let file_size = source
        .as_path()
        .and_then(|fs_path| std::fs::metadata(fs_path).ok())
        .map_or(0, |metadata| metadata.len());

    // Create per-file progress bar if the file is large enough
    let file_progress =
        progress_manager.create_file_progress(input_file.to_string_lossy().as_ref(), file_size);

    // Perform verification using DataSource
    let chunk_size_kb = convert_chunk_size_kb(chunk_size_kb);
    let computed_hash = match file_progress {
        Some(progress) => {
            // With progress callback
            source.hash(
                algo,
                chunk_size_kb,
                parallel_readers,
                Some(move |bytes_read| {
                    progress.update(bytes_read);
                }),
            )
        }
        None => {
            // Without progress callback
            source.hash(algo, chunk_size_kb, parallel_readers, None::<fn(u64)>)
        }
    };

    let computed_hash = match computed_hash {
        Ok(hash) => hash,
        Err(e) => {
            let result = VerificationResult::new_error(
                input_file.to_path_buf(),
                hash_to_verify.to_string(),
                e.to_string(),
            );
            display_verification_table(&[result])?;
            return Err(e);
        }
    };

    let passed = computed_hash == hash_to_verify;

    // Get file metadata for display - try filesystem metadata if available
    let result = if let Some(fs_path) = source.as_path() {
        if let Ok(metadata) = std::fs::metadata(fs_path) {
            VerificationResult::new_with_metadata(
                input_file.to_path_buf(),
                hash_to_verify.to_string(),
                computed_hash,
                passed,
                &metadata,
            )?
        } else {
            VerificationResult::new(
                input_file.to_path_buf(),
                hash_to_verify.to_string(),
                computed_hash,
                passed,
            )?
        }
    } else {
        // Archive source - no filesystem metadata available
        VerificationResult::new(
            input_file.to_path_buf(),
            hash_to_verify.to_string(),
            computed_hash,
            passed,
        )?
    };

    display_verification_table(slice::from_ref(&result))?;

    // Finish progress display
    progress_manager.inc_overall();
    progress_manager.finish_with_message("Verification completed");

    if !passed {
        return Err(CheckleError::FailedChecksum(input_file.to_path_buf()));
    }

    Ok(())
}

/// Verify a file without pretty output (simple pass/fail).
fn verify_simple(
    input_file: &Path,
    hash_to_verify: &str,
    algo: HashingAlgo,
    no_progress: NoProgress,
    chunk_size_kb: usize,
    parallel_readers: usize,
) -> Result<()> {
    let source = create_data_source_from_path(input_file)?;

    // Create progress manager (single file verification)
    let show_progress = !no_progress;
    let progress_manager = ProgressManager::new(show_progress, 1);

    // Get file size for progress tracking
    let file_size = source
        .as_path()
        .and_then(|fs_path| std::fs::metadata(fs_path).ok())
        .map_or(0, |metadata| metadata.len());

    // Create per-file progress bar if the file is large enough
    let file_progress =
        progress_manager.create_file_progress(input_file.to_string_lossy().as_ref(), file_size);

    let chunk_size_kb = convert_chunk_size_kb(chunk_size_kb);

    // Perform verification with optional progress callback
    match file_progress {
        Some(progress) => {
            // With progress callback
            let computed_hash = source.hash(
                algo,
                chunk_size_kb,
                parallel_readers,
                Some(move |bytes_read| {
                    progress.update(bytes_read);
                }),
            )?;

            if computed_hash != hash_to_verify {
                return Err(CheckleError::FailedChecksum(input_file.to_path_buf()));
            }
        }
        None => {
            // Without progress callback - use the existing verify method
            source.verify(hash_to_verify, algo, chunk_size_kb, parallel_readers)?;
        }
    }

    // Finish progress display
    progress_manager.inc_overall();
    progress_manager.finish_with_message("Verification completed");

    Ok(())
}

// Helper functions that were in utils module

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
    // Support both formats:
    // 1. Just the hash: "d41d8cd98f00b204e9800998ecf8427e"
    // 2. Hash with filename: "d41d8cd98f00b204e9800998ecf8427e  filename.txt"
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
