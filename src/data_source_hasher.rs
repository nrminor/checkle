//! Wrapper module that enables hashing of `DataSource` using the existing Hasher.
//!
//! This module provides functions to hash `DataSource` instances without modifying
//! the core Hasher struct, maintaining full backward compatibility.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{
    data_source::DataSource,
    errors::{CheckleError, Result},
    hashing::{Hasher, HashingAlgo},
};

/// Hash a `DataSource` using the specified algorithm.
///
/// This function provides a bridge between `DataSource` and the existing Hasher
/// implementation. For filesystem sources, it uses the optimized Hasher directly.
/// For archive sources, it extracts the data and hashes it sequentially.
///
/// # Arguments
///
/// * `source` - The data source to hash
/// * `algo` - The hashing algorithm to use
/// * `chunk_size_kb` - Chunk size in KB (0 for default)
/// * `parallel_readers` - Number of parallel readers (filesystem only)
/// * `progress_callback` - Optional progress callback
///
/// # Returns
///
/// The computed hash as a hexadecimal string.
///
/// # Errors
///
/// Returns an error if:
/// - The source cannot be opened
/// - Reading fails
/// - Hash computation fails
pub fn hash_data_source<F>(
    source: DataSource,
    algo: HashingAlgo,
    chunk_size_kb: u16,
    parallel_readers: usize,
    progress_callback: Option<F>,
) -> Result<String>
where
    F: Fn(u64) + Send + Sync + 'static,
{
    match source {
        DataSource::Filesystem { path } => {
            // For filesystem sources, use the existing optimized Hasher
            hash_filesystem_source(
                path.as_path(),
                algo,
                chunk_size_kb,
                parallel_readers,
                progress_callback,
            )
        }
        DataSource::Archive { .. } => {
            // For archive sources, use sequential hashing
            hash_archive_source(&source, algo, chunk_size_kb, progress_callback)
        }
    }
}

/// Hash a filesystem source using the existing Hasher.
fn hash_filesystem_source<F>(
    path: &Path,
    algo: HashingAlgo,
    chunk_size_kb: u16,
    parallel_readers: usize,
    progress_callback: Option<F>,
) -> Result<String>
where
    F: Fn(u64) + Send + Sync + 'static,
{
    let hasher = match algo {
        HashingAlgo::Md5 => {
            let mut h = Hasher::new_md5(path);
            if chunk_size_kb > 0 {
                h = h.with_chunk_size((chunk_size_kb as usize) * 1024)?;
            }
            if parallel_readers > 0 {
                h = h.with_parallel_readers(parallel_readers);
            }
            if let Some(callback) = progress_callback {
                h = h.with_progress_callback(callback);
            }
            h.find_root_hash()
        }
        HashingAlgo::Sha2 => {
            let mut h = Hasher::new_sha2(path);
            if chunk_size_kb > 0 {
                h = h.with_chunk_size((chunk_size_kb as usize) * 1024)?;
            }
            if parallel_readers > 0 {
                h = h.with_parallel_readers(parallel_readers);
            }
            if let Some(callback) = progress_callback {
                h = h.with_progress_callback(callback);
            }
            h.find_root_hash()
        }
    }?;

    Ok(hasher)
}

/// Hash an archive source by extracting and hashing sequentially.
#[allow(clippy::needless_pass_by_value)]
fn hash_archive_source<F>(
    source: &DataSource,
    algo: HashingAlgo,
    chunk_size_kb: u16,
    progress_callback: Option<F>,
) -> Result<String>
where
    F: Fn(u64) + Send + Sync + 'static,
{
    use md5::{Digest as Md5Digest, Md5};
    use sha2::{Digest, Sha256};

    let mut reader = source.open_reader()?;
    let chunk_size = if chunk_size_kb > 0 {
        (chunk_size_kb as usize) * 1024
    } else {
        crate::constants::DEFAULT_CHUNK_SIZE
    };

    let mut buffer = vec![0u8; chunk_size];
    let mut total_read = 0u64;

    match algo {
        HashingAlgo::Md5 => {
            let mut hasher = Md5::new();

            loop {
                let bytes_read =
                    reader
                        .read(&mut buffer)
                        .map_err(|e| CheckleError::FileReadError {
                            path: PathBuf::from(source.display_path()),
                            source: e,
                        })?;

                if bytes_read == 0 {
                    break;
                }

                Digest::update(&mut hasher, &buffer[..bytes_read]);
                total_read += bytes_read as u64;

                if let Some(ref callback) = progress_callback {
                    callback(total_read);
                }
            }

            Ok(format!("{:x}", hasher.finalize()))
        }
        HashingAlgo::Sha2 => {
            let mut hasher = Sha256::new();

            loop {
                let bytes_read =
                    reader
                        .read(&mut buffer)
                        .map_err(|e| CheckleError::FileReadError {
                            path: PathBuf::from(source.display_path()),
                            source: e,
                        })?;

                if bytes_read == 0 {
                    break;
                }

                Digest::update(&mut hasher, &buffer[..bytes_read]);
                total_read += bytes_read as u64;

                if let Some(ref callback) = progress_callback {
                    callback(total_read);
                }
            }

            Ok(format!("{:x}", hasher.finalize()))
        }
    }
}

/// Verify a `DataSource` against an expected hash.
///
/// # Arguments
///
/// * `source` - The data source to verify
/// * `expected_hash` - The expected hash value
/// * `algo` - The hashing algorithm to use
/// * `chunk_size_kb` - Chunk size in KB (0 for default)
/// * `parallel_readers` - Number of parallel readers (filesystem only)
///
/// # Returns
///
/// Ok(()) if the hash matches, error otherwise.
///
/// # Errors
///
/// Returns an error if:
/// - The source cannot be read
/// - Hash computation fails
/// - The computed hash doesn't match the expected hash
pub fn verify_data_source(
    source: &DataSource,
    expected_hash: &str,
    algo: HashingAlgo,
    chunk_size_kb: u16,
    parallel_readers: usize,
) -> Result<()> {
    let computed_hash = hash_data_source(
        source.clone(),
        algo,
        chunk_size_kb,
        parallel_readers,
        None::<fn(u64)>,
    )?;

    if computed_hash == expected_hash {
        Ok(())
    } else {
        Err(CheckleError::FailedChecksum(PathBuf::from(
            source.display_path(),
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_filesystem_source() {
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"Hello, world!").unwrap();

        let source = DataSource::from_path(temp_file.path().to_path_buf());
        let hash = hash_data_source(source, HashingAlgo::Md5, 0, 0, None::<fn(u64)>).unwrap();

        // Known MD5 of "Hello, world!"
        assert_eq!(hash, "6cd3556deb0da54bca060b4c39479839");
    }

    #[test]
    fn test_hash_with_progress() {
        use std::sync::{Arc, Mutex};

        let temp_file = NamedTempFile::new().unwrap();
        let content = vec![0u8; 10_000]; // 10KB
        std::fs::write(temp_file.path(), &content).unwrap();

        let source = DataSource::from_path(temp_file.path().to_path_buf());
        let progress_called = Arc::new(Mutex::new(false));
        let progress_called_clone = Arc::clone(&progress_called);

        let callback = move |_bytes: u64| {
            if let Ok(mut called) = progress_called_clone.lock() {
                *called = true;
            }
        };

        let _hash = hash_data_source(source, HashingAlgo::Sha2, 4, 0, Some(callback)).unwrap();

        if let Ok(called) = progress_called.lock() {
            assert!(*called, "Progress callback should be called");
        }
    }

    #[test]
    fn test_verify_success() {
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"test data").unwrap();

        let source = DataSource::from_path(temp_file.path().to_path_buf());

        // First compute the hash
        let hash =
            hash_data_source(source.clone(), HashingAlgo::Md5, 0, 0, None::<fn(u64)>).unwrap();

        // Then verify it
        let result = verify_data_source(&source, &hash, HashingAlgo::Md5, 0, 0);
        assert!(result.is_ok(), "Verification should succeed");
    }

    #[test]
    fn test_verify_failure() {
        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"test data").unwrap();

        let source = DataSource::from_path(temp_file.path().to_path_buf());

        // Try to verify with wrong hash
        let result = verify_data_source(&source, "wronghash", HashingAlgo::Md5, 0, 0);
        assert!(result.is_err(), "Verification should fail");
    }
}
