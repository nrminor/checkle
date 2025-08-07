//! Data source abstraction for unified file and archive entry access.
//!
//! This module provides a unified interface for accessing data from both
//! filesystem files and archive entries, allowing the hashing system to
//! work seamlessly with both types of inputs.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

#[cfg(feature = "tar")]
use crate::archive::TarArchive;
#[cfg(feature = "zip")]
use crate::archive::ZipArchive;
use crate::archive::{ArchiveReader, MAX_ARCHIVE_SIZE};
use crate::archive_path::ArchivePathComponents;
use crate::constants::MAX_PATH_LENGTH;
use crate::errors::{CheckleError, Result};
use crate::hashing::{Hasher, HashingAlgo};

/// Represents the source of data for hashing operations.
#[derive(Debug, Clone)]
pub enum DataSource {
    /// Data from a filesystem file
    Filesystem {
        /// Path to the file
        path: PathBuf,
    },
    /// Data from within an archive
    Archive {
        /// Path to the archive file
        archive_path: PathBuf,
        /// Path within the archive
        entry_path: String,
    },
}

impl DataSource {
    /// Create a filesystem data source.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file
    ///
    /// # Returns
    ///
    /// A new `DataSource::Filesystem` variant
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The path doesn't exist
    /// - The path is empty
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        assert!(!path.as_os_str().is_empty(), "Path must not be empty");
        assert!(path.exists(), "File must exist: {}", path.display());

        Self::Filesystem { path }
    }

    /// Create an archive data source from archive path components.
    ///
    /// # Arguments
    ///
    /// * `components` - Parsed archive path components
    ///
    /// # Returns
    ///
    /// A new `DataSource::Archive` variant or an error
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The archive doesn't exist
    /// - The archive is too large
    /// - The archive path is invalid
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The archive entry path is empty
    /// - The entry path exceeds the maximum allowed length
    pub fn from_archive(components: &ArchivePathComponents) -> Result<Self> {
        // Validate archive exists
        if !components.archive().exists() {
            return Err(CheckleError::InaccessibleFile(
                components.archive().to_path_buf(),
            ));
        }

        // Check file size limits
        let metadata =
            std::fs::metadata(components.archive()).map_err(|e| CheckleError::FileReadError {
                path: components.archive().to_path_buf(),
                source: e,
            })?;

        if metadata.len() > MAX_ARCHIVE_SIZE {
            return Err(CheckleError::ArchiveTooLarge {
                path: components.archive().to_path_buf(),
                size: metadata.len(),
                limit: MAX_ARCHIVE_SIZE,
            });
        }

        // Additional validation
        let entry_path = components.entry();
        assert!(
            !entry_path.is_empty(),
            "Archive entry path must not be empty"
        );
        assert!(
            entry_path.len() <= MAX_PATH_LENGTH,
            "Entry path exceeds maximum length"
        );

        Ok(Self::Archive {
            archive_path: components.archive().to_path_buf(),
            entry_path: entry_path.to_string(),
        })
    }

    /// Get a reader for this data source.
    ///
    /// # Returns
    ///
    /// A boxed reader that can read the data
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - The archive cannot be opened
    /// - The entry doesn't exist in the archive
    pub fn open_reader(&self) -> Result<Box<dyn Read>> {
        match self {
            Self::Filesystem { path } => {
                let file = File::open(path).map_err(|e| CheckleError::FileOpenError {
                    path: path.clone(),
                    source: e,
                })?;
                Ok(Box::new(BufReader::new(file)))
            }
            Self::Archive {
                archive_path,
                entry_path,
            } => {
                // For now, open the archive directly based on type
                // TODO: Refactor to use a trait object once ArchiveReader is made object-safe
                let path = std::path::Path::new(archive_path);

                #[cfg(feature = "tar")]
                if is_tar_archive(path) {
                    let mut archive = TarArchive::open(archive_path)?;
                    return match archive.find_entry(entry_path)? {
                        Some((entry_reader, _metadata)) => Ok(Box::new(entry_reader)),
                        None => Err(CheckleError::ArchiveEntryNotFound {
                            archive: archive_path.clone(),
                            entry: entry_path.clone(),
                        }),
                    };
                }

                #[cfg(feature = "zip")]
                if is_zip_archive(path) {
                    let mut archive = ZipArchive::open(archive_path)?;
                    return match archive.find_entry(entry_path)? {
                        Some((entry_reader, _metadata)) => Ok(Box::new(entry_reader)),
                        None => Err(CheckleError::ArchiveEntryNotFound {
                            archive: archive_path.clone(),
                            entry: entry_path.clone(),
                        }),
                    };
                }

                Err(CheckleError::UnsupportedArchiveFormat(archive_path.clone()))
            }
        }
    }

    /// Get display path for logging/errors.
    ///
    /// For filesystem sources, returns the path as-is.
    /// For archive sources, returns "archive.tar:entry/path" format.
    #[must_use]
    pub fn display_path(&self) -> String {
        match self {
            Self::Filesystem { path } => path.display().to_string(),
            Self::Archive {
                archive_path,
                entry_path,
            } => format!("{}:{}", archive_path.display(), entry_path),
        }
    }

    /// Check if this data source exists/is accessible.
    ///
    /// # Returns
    ///
    /// `true` if the source exists and is accessible, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if checking existence fails (e.g., archive corruption)
    pub fn exists(&self) -> Result<bool> {
        match self {
            Self::Filesystem { path } => Ok(path.exists()),
            Self::Archive {
                archive_path,
                entry_path,
            } => {
                if !archive_path.exists() {
                    return Ok(false);
                }

                let path = std::path::Path::new(archive_path);

                #[cfg(feature = "tar")]
                if is_tar_archive(path) {
                    let mut archive = TarArchive::open(archive_path)?;
                    return Ok(archive.find_entry(entry_path)?.is_some());
                }

                #[cfg(feature = "zip")]
                if is_zip_archive(path) {
                    let mut archive = ZipArchive::open(archive_path)?;
                    return Ok(archive.find_entry(entry_path)?.is_some());
                }

                Ok(false)
            }
        }
    }

    /// Get the path for filesystem sources.
    ///
    /// Returns `None` for archive sources.
    #[must_use]
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Filesystem { path } => Some(path),
            Self::Archive { .. } => None,
        }
    }

    /// Check if this is an archive source.
    #[must_use]
    pub fn is_archive(&self) -> bool {
        matches!(self, Self::Archive { .. })
    }

    /// Get file size if available.
    ///
    /// For filesystem sources, returns the file size.
    /// For archive sources, attempts to get the uncompressed size from metadata.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata cannot be accessed
    pub fn file_size(&self) -> Result<Option<u64>> {
        match self {
            Self::Filesystem { path } => {
                let metadata =
                    std::fs::metadata(path).map_err(|e| CheckleError::FileReadError {
                        path: path.clone(),
                        source: e,
                    })?;
                Ok(Some(metadata.len()))
            }
            Self::Archive {
                archive_path,
                entry_path,
            } => {
                let path = std::path::Path::new(archive_path);

                #[cfg(feature = "tar")]
                if is_tar_archive(path) {
                    let mut archive = TarArchive::open(archive_path)?;
                    return match archive.find_entry(entry_path)? {
                        Some((_entry, metadata)) => Ok(Some(metadata.size)),
                        None => Ok(None),
                    };
                }

                #[cfg(feature = "zip")]
                if is_zip_archive(path) {
                    let mut archive = ZipArchive::open(archive_path)?;
                    return match archive.find_entry(entry_path)? {
                        Some((_entry, metadata)) => Ok(Some(metadata.size)),
                        None => Ok(None),
                    };
                }

                Ok(None)
            }
        }
    }

    /// Hash this data source using the specified algorithm.
    ///
    /// This method provides optimized hashing for both filesystem and archive sources.
    /// For filesystem sources, it uses the optimized parallel Hasher when applicable.
    /// For archive sources, it performs sequential hashing.
    ///
    /// # Arguments
    ///
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
    pub fn hash<F>(
        &self,
        algo: HashingAlgo,
        chunk_size_kb: u16,
        parallel_readers: usize,
        progress_callback: Option<F>,
    ) -> Result<String>
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        match self {
            Self::Filesystem { path } => {
                // For filesystem sources, use the existing optimized Hasher
                hash_filesystem_source(
                    path.as_path(),
                    algo,
                    chunk_size_kb,
                    parallel_readers,
                    progress_callback,
                )
            }
            Self::Archive { .. } => {
                // For archive sources, use sequential hashing
                hash_archive_source(self, algo, chunk_size_kb, progress_callback)
            }
        }
    }

    /// Verify this data source against an expected hash.
    ///
    /// # Arguments
    ///
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
    pub fn verify(
        &self,
        expected_hash: &str,
        algo: HashingAlgo,
        chunk_size_kb: u16,
        parallel_readers: usize,
    ) -> Result<()> {
        let computed_hash = self.hash(algo, chunk_size_kb, parallel_readers, None::<fn(u64)>)?;

        if computed_hash == expected_hash {
            Ok(())
        } else {
            Err(CheckleError::FailedChecksum(PathBuf::from(
                self.display_path(),
            )))
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

/// Check if the given path is a TAR archive based on extension.
fn is_tar_archive(path: &std::path::Path) -> bool {
    let has_tar_extension = if let Some(extension) = path.extension() {
        let ext_str = extension.to_string_lossy().to_lowercase();
        ext_str == "tar" || ext_str == "tgz"
    } else {
        false
    };

    has_tar_extension || path.to_string_lossy().to_lowercase().contains(".tar.")
}

/// Check if the given path is a ZIP archive based on extension.
fn is_zip_archive(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_filesystem_data_source_creation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let source = DataSource::from_path(file_path.clone());
        assert!(matches!(source, DataSource::Filesystem { .. }));
        assert_eq!(source.display_path(), file_path.display().to_string());
        assert!(!source.is_archive());
        assert_eq!(source.as_path(), Some(file_path.as_path()));
    }

    #[test]
    #[should_panic(expected = "File must exist")]
    fn test_filesystem_data_source_nonexistent() {
        let path = PathBuf::from("/nonexistent/file.txt");
        let _ = DataSource::from_path(path);
    }

    #[test]
    fn test_archive_data_source_validation() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar");
        std::fs::write(&archive_path, b"dummy archive").unwrap();

        let components = ArchivePathComponents::new(
            archive_path.clone(),
            crate::archive_path::ArchivePattern::SpecificFile("file.txt".to_string()),
        )
        .unwrap();

        let result = DataSource::from_archive(&components);
        assert!(result.is_ok());

        let source = result.unwrap();
        assert!(source.is_archive());
        assert_eq!(
            source.display_path(),
            format!("{}:file.txt", archive_path.display())
        );
        assert!(source.as_path().is_none());
    }

    #[test]
    fn test_data_source_reader_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = b"Hello, world!";
        std::fs::write(&file_path, content).unwrap();

        let source = DataSource::from_path(file_path);
        let mut reader = source.open_reader().unwrap();

        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).unwrap();
        assert_eq!(&buffer, content);
    }

    #[test]
    fn test_data_source_file_size() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = b"Hello, world!";
        std::fs::write(&file_path, content).unwrap();

        let source = DataSource::from_path(file_path);
        let size = source.file_size().unwrap();
        assert_eq!(size, Some(content.len() as u64));
    }

    #[test]
    fn test_data_source_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, b"test").unwrap();

        let source = DataSource::from_path(file_path.clone());
        assert!(source.exists().unwrap());

        // Remove file and check again
        std::fs::remove_file(&file_path).unwrap();
        let source_nonexistent = DataSource::Filesystem { path: file_path };
        assert!(!source_nonexistent.exists().unwrap());
    }

    #[test]
    fn test_hash_filesystem_source() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, b"Hello, world!").unwrap();

        let source = DataSource::from_path(file_path);
        let hash = source
            .hash(HashingAlgo::Md5, 0, 0, None::<fn(u64)>)
            .unwrap();

        // Known MD5 of "Hello, world!"
        assert_eq!(hash, "6cd3556deb0da54bca060b4c39479839");
    }

    #[test]
    fn test_hash_with_progress() {
        use std::sync::{Arc, Mutex};

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = vec![0u8; 10_000]; // 10KB
        std::fs::write(&file_path, &content).unwrap();

        let source = DataSource::from_path(file_path);
        let progress_called = Arc::new(Mutex::new(false));
        let progress_called_clone = Arc::clone(&progress_called);

        let callback = move |_bytes: u64| {
            if let Ok(mut called) = progress_called_clone.lock() {
                *called = true;
            }
        };

        let _hash = source
            .hash(HashingAlgo::Sha2, 4, 0, Some(callback))
            .unwrap();

        if let Ok(called) = progress_called.lock() {
            assert!(*called, "Progress callback should be called");
        }
    }

    #[test]
    fn test_verify_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, b"test data").unwrap();

        let source = DataSource::from_path(file_path);

        // First compute the hash
        let hash = source
            .hash(HashingAlgo::Md5, 0, 0, None::<fn(u64)>)
            .unwrap();

        // Then verify it
        let result = source.verify(&hash, HashingAlgo::Md5, 0, 0);
        assert!(result.is_ok(), "Verification should succeed");
    }

    #[test]
    fn test_verify_failure() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, b"test data").unwrap();

        let source = DataSource::from_path(file_path);

        // Try to verify with wrong hash
        let result = source.verify("wronghash", HashingAlgo::Md5, 0, 0);
        assert!(result.is_err(), "Verification should fail");
    }
}
