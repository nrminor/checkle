//! Data source abstraction for unified file and archive entry access.
//!
//! This module provides a unified interface for accessing data from both
//! filesystem files and archive entries, allowing the hashing system to
//! work seamlessly with both types of inputs.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use crate::archive::{ArchiveReader, MAX_ARCHIVE_SIZE};
use crate::archive_path::ArchivePathComponents;
use crate::constants::MAX_PATH_LENGTH;
use crate::errors::CheckleError;

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
    pub fn from_archive(components: &ArchivePathComponents) -> Result<Self, CheckleError> {
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
    pub fn open_reader(&self) -> Result<Box<dyn Read>, CheckleError> {
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
                    use crate::archive::TarArchive;
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
                    use crate::archive::ZipArchive;
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
    pub fn exists(&self) -> Result<bool, CheckleError> {
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
                    use crate::archive::TarArchive;
                    let mut archive = TarArchive::open(archive_path)?;
                    return Ok(archive.find_entry(entry_path)?.is_some());
                }

                #[cfg(feature = "zip")]
                if is_zip_archive(path) {
                    use crate::archive::ZipArchive;
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
    pub fn file_size(&self) -> Result<Option<u64>, CheckleError> {
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
                    use crate::archive::TarArchive;
                    let mut archive = TarArchive::open(archive_path)?;
                    return match archive.find_entry(entry_path)? {
                        Some((_entry, metadata)) => Ok(Some(metadata.size)),
                        None => Ok(None),
                    };
                }

                #[cfg(feature = "zip")]
                if is_zip_archive(path) {
                    use crate::archive::ZipArchive;
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

        let components =
            ArchivePathComponents::new(archive_path.clone(), "file.txt".to_string()).unwrap();

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
}
