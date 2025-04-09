use log::{debug, warn};
use std::{
    env,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::prelude::CheckleError;

pub struct FilesToCheck(Vec<FileHashPair>);

impl FilesToCheck {
    /// A specialized container type intended for holding `FileHashPair`s - a paired `PathBuf` and hash value - for verification.
    /// This container specifically supports checksum verification workflows by managing collections of
    /// files and their expected hash values.
    ///
    /// # Description
    /// `FilesToCheck` wraps a `Vec<FileHashPair>` and provides methods for creating, manipulating, and
    /// processing collections of files with their associated hash values. It's primarily used for
    /// verification operations where a set of files need to be checked against known checksums.
    ///
    /// # Examples
    /// Files can be added individually using `push()`, created empty with `new()`, or constructed from
    /// an existing vector using `from_vec()`. The container can also be built from a checksum file
    /// using `new_from_txt()`.
    ///
    /// When done, the internal vector can be extracted using `to_vec()`.
    ///
    /// # Features
    /// - Stores file paths and their expected hash values
    /// - Supports sequential processing of file-hash pairs
    /// - Provides conversion methods to and from `Vec<FileHashPair>`
    /// - Can be constructed from checksum files
    ///
    /// # Errors
    /// Most methods on this type don't produce errors directly, but `new_from_txt()` can fail when:
    /// - The checksum file is inaccessible
    /// - The checksum file format is invalid
    /// - Referenced files don't exist (produces warnings)
    ///
    /// # Panics
    /// This type's methods don't panic under normal circumstances.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a `FilesToCheck` from a vector of `FileHashPair`s.
    ///
    /// # Description
    /// This is a convenience constructor that allows creating a `FilesToCheck` instance directly from
    /// an existing vector of `FileHashPair`s. This provides an alternative to building the collection
    /// one item at a time using `push()`.
    ///
    /// # Examples
    /// This is commonly used when you already have a collection of files and their hashes, perhaps
    /// from another source:
    /// ```no_run
    /// # use std::path::PathBuf;
    /// # use checkle::FilesToCheck;
    /// # use checkle::FileHashPair;
    /// let pairs = vec![FileHashPair::new(PathBuf::from("file.txt"), "hash123".to_string())];
    /// let files = FilesToCheck::from_vec(pairs);
    /// ```
    ///
    /// # Arguments
    /// * `pairs` - A vector of `FileHashPair` instances to initialize the collection with
    ///
    /// # Returns
    /// Returns a new `FilesToCheck` instance containing the provided pairs
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn from_vec(pairs: Vec<FileHashPair>) -> Self {
        Self(pairs)
    }

    /// Consumes this `FilesToCheck` and returns the contained vector of `FileHashPair`s.
    ///
    /// # Description
    /// This method provides access to the underlying storage of file-hash pairs by consuming
    /// the `FilesToCheck` container and returning ownership of the internal vector. This is useful
    /// when you need to process the pairs outside of the `FilesToCheck` context or when integrating
    /// with code that expects a `Vec<FileHashPair>`.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// A `Vec<FileHashPair>` containing all the file-hash pairs that were stored in this container.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn to_vec(self) -> Vec<FileHashPair> {
        self.0
    }

    /// Adds a single `FileHashPair` to this collection.
    ///
    /// # Description
    /// This method adds a file path and hash value pair to the collection for later verification.
    /// The pair is appended to the end of the internal vector.
    ///
    /// # Arguments
    /// * `item` - A `FileHashPair` containing a file path and its expected hash value
    ///
    /// # Returns
    /// None - this method modifies the collection in place
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    pub fn push(&mut self, item: FileHashPair) {
        self.0.push(item);
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
    /// Creates a new `FileHashPair` from a file path and hash value.
    ///
    /// # Description
    /// This method constructs a new `FileHashPair` that associates a file path with its expected hash value.
    /// These pairs are typically used for file verification workflows where actual file hashes are compared
    /// against expected values.
    ///
    /// # Arguments
    /// * `file` - A `PathBuf` representing the path to the file to be checked
    /// * `hash` - A `String` containing the expected hash value for the file
    ///
    /// # Returns
    /// Returns a new `FileHashPair` instance containing the provided file path and hash value.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn new(file: PathBuf, hash: String) -> Self {
        Self { file, hash }
    }

    /// Returns a reference to the file path component of this `FileHashPair`.
    ///
    /// # Description
    /// This method provides read-only access to the file path stored in this pair. This is typically
    /// used when you need to inspect or use the file path without taking ownership of it, such as
    /// during verification operations.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// Returns a reference to the `Path` representing the file location.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Returns a reference to the hash value component of this `FileHashPair`.
    ///
    /// # Description
    /// This method provides read-only access to the expected hash value stored in this pair. This is typically
    /// used during verification operations where the stored hash needs to be compared against a computed
    /// hash value for the associated file.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// Returns a reference to the hash value as a string slice.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Returns a mutable reference to the file path component of this `FileHashPair`.
    ///
    /// # Description
    /// This method provides mutable access to the file path stored in this pair. It allows modifying
    /// the path when needed, such as during path normalization or when updating file locations.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// Returns a mutable reference to the `Path` representing the file location.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn file_mut(&mut self) -> &mut Path {
        &mut self.file
    }
    /// Returns a mutable reference to the hash value component of this `FileHashPair`.
    ///
    /// # Description
    /// This method provides mutable access to the hash value stored in this pair. It allows modifying
    /// the expected hash value when needed, such as during hash value normalization or when updating
    /// verification data.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// Returns a mutable reference to the hash value as a string slice.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    pub fn hash_mut(&mut self) -> &mut str {
        &mut self.hash
    }
    /// Returns a tuple containing owned versions of the file path and hash value.
    ///
    /// # Description
    /// This method consumes the `FileHashPair` and returns its components as a tuple containing
    /// the owned `PathBuf` and `String`. This is useful when you need to take ownership of both
    /// components, such as when passing them to functions that require owned values or when
    /// splitting the pair for separate processing.
    ///
    /// # Arguments
    /// None
    ///
    /// # Returns
    /// Returns a tuple `(PathBuf, String)` containing the file path and hash value.
    ///
    /// # Errors
    /// This method cannot fail and does not return any errors.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    #[must_use]
    pub fn file_hash_owned(self) -> (PathBuf, String) {
        (self.file, self.hash)
    }
}

impl FilesToCheck {
    /// Creates a new `FilesToCheck` from a text file containing file paths and their checksums.
    ///
    /// # Description
    /// This method parses a checksum file where each line contains a file path and its expected hash value,
    /// separated by a tab character. It validates the existence of each referenced file and constructs a
    /// collection of `FileHashPair`s for verification.
    ///
    /// Lines with files that don't exist will be skipped with a warning. The checksum file must have
    /// exactly two tab-separated fields per line (file path and hash).
    ///
    /// # Arguments
    /// * `checksum_file` - Path to the text file containing file paths and checksums
    ///
    /// # Returns
    /// Returns a `Result` containing either a new `FilesToCheck` instance or a `CheckleError`.
    ///
    /// # Errors
    /// Returns `CheckleError::InaccessibleFile` if the checksum file cannot be opened.
    /// Returns `CheckleError::InvalidChecksumFile` if the file format is invalid.
    ///
    /// # Panics
    /// This method does not panic under any circumstances.
    pub fn new_from_txt(checksum_file: &Path) -> Result<FilesToCheck, CheckleError> {
        let Ok(file_handle) = File::open(checksum_file) else {
            return Err(CheckleError::InaccessibleFile(checksum_file.to_path_buf()));
        };
        let buffer = BufReader::new(file_handle);

        let mut files_to_check = FilesToCheck::new();
        for line in buffer.lines() {
            let Ok(line) = line else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };

            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 2 {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            }

            let (hash, file_str) = (fields[0], fields[1]);
            let file_path = PathBuf::from(file_str);
            if !file_path.exists() {
                warn!("A file listed in the checksum file, {file_str}, does not exist and will be skipped");
                continue;
            }

            let wrapper = FileHashPair::new(file_path, hash.to_string());

            files_to_check.push(wrapper);
        }

        Ok(files_to_check)
    }
}

/// Collects files for checksum verification based on a given path pattern.
///
/// # Description
/// This function takes a path input and returns a vector of file paths to be processed. It handles
/// special path patterns like "*", "./*", "./", and "." to collect all files in the current directory,
/// or returns a single file path otherwise. This is typically used to gather files that need checksum
/// verification.
///
/// # Arguments
/// * `input` - A path specifying either a single file or a pattern to match multiple files
///
/// # Returns
/// Returns a `Vec<PathBuf>` containing one or more file paths to be processed:
/// - For special patterns, returns paths of all files in the current directory
/// - For specific paths, returns a single-element vector with that path
///
/// # Errors
/// This function handles errors internally and will skip any unreadable directory entries.
///
/// # Panics
/// May panic if the current directory cannot be accessed or read when using pattern matching.
#[must_use]
pub fn collect_files(input: &Path) -> Vec<PathBuf> {
    if input == PathBuf::from("*")
        || input == PathBuf::from("./*")
        || input == PathBuf::from("./")
        || input == PathBuf::from(".")
    {
        let current_dir = env::current_dir().unwrap();
        let entries = fs::read_dir(current_dir).unwrap();
        let mut file_paths = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                file_paths.push(path);
            }
        }

        debug!("Preparing to hash {} files...", file_paths.len());
        file_paths
    } else {
        let wrapped_file = vec![input.to_path_buf()];
        debug!("Preparing to hash {} file(s)...", wrapped_file.len());
        wrapped_file
    }
}
