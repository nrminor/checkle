use color_eyre::eyre::Result;
use log::{debug, info, warn};
use md5::Md5;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    marker::Sized,
    path::Path,
};

use crate::{
    io::FilesToCheck,
    prelude::CheckleError::{self, *},
};

// set the chunk size to be 1 megabyte
const CHUNK_SIZE: usize = 1024 * 1024;

// defining the hash sizes for the two supported algorithms
const MD5_SIZE: usize = 16;
const SHA_SIZE: usize = 32;

/// Represents supported hashing algorithms and selects between using MD5 (fast but less secure)
/// and SHA256 (slower but cryptographically secure) for computing Merkle trees and file checksums.
///
/// This enum is used throughout the crate to allow dynamic selection of the hashing algorithm
/// when verifying file integrity and generating Merkle tree hashes. The default algorithm is MD5.
///
/// # Examples
///
/// The algorithm is typically selected when instantiating a new [`Hasher`] or when calling
/// [`FilesToCheck::checksum_all()`].
#[derive(Debug, Default, Clone)]
pub enum HashingAlgo {
    #[default]
    Md5,
    Sha2,
}

/// A struct that manages file content verification through hashing algorithms, supporting both MD5 and SHA256.
///
/// This struct is the core component for generating Merkle trees from file contents and computing
/// root hashes for integrity verification. It works with two possible hash sizes (16 bytes for MD5 and
/// 32 bytes for SHA256) and processes files in 1MB chunks for memory efficiency.
///
/// The hasher operates by:
/// 1. Reading the input file in chunks
/// 2. Computing individual hashes for each chunk
/// 3. Building a Merkle tree from those hashes
/// 4. Computing a final root hash that can be used for verification
///
/// # Type Parameters
///
/// * `'a` - The lifetime of the referenced path
/// * `N` - A const generic parameter representing the hash size in bytes (16 for MD5, 32 for SHA256)
///
/// # Arguments
///
/// * `path` - A reference to the path of the file to be hashed
/// * `algorithm` - The hashing algorithm to use (MD5 or SHA256)
///
/// # Errors
///
/// This struct's methods may return errors in the following situations:
/// * File I/O errors when reading the target file
/// * UTF-8 conversion errors when converting hashes to strings
/// * Hash size mismatch errors if the implementation doesn't match the const generic parameter
///
/// # Examples
///
/// The struct is typically used through its convenience constructors:
/// * `Hasher::new_md5()` for MD5 hashing
/// * `Hasher::new_sha2()` for SHA256 hashing
pub struct Hasher<'a, const N: usize> {
    pub path: &'a Path,
    pub algorithm: HashingAlgo,
}

impl<'a> Hasher<'a, MD5_SIZE> {
    /// Creates a new `Hasher` instance configured to use the MD5 hashing algorithm.
    ///
    /// This is a convenience constructor that configures the hasher with the MD5 algorithm,
    /// which is faster but cryptographically weaker than SHA256. The hasher will process
    /// the target file in 1MB chunks to build a Merkle tree of MD5 hashes.
    ///
    /// MD5 produces 16-byte (128-bit) hashes and is suitable for file integrity verification
    /// in non-security-critical contexts where performance is prioritized over cryptographic
    /// security. For security-critical applications, prefer [`Hasher::new_sha2()`].
    ///
    /// This method enforces the correct hash size using Rust's const generics system,
    /// making it impossible to create an MD5 hasher with the wrong output size.
    ///
    /// # Arguments
    ///
    /// * `path` - A reference to the path of the file to be hashed, which must live
    ///   at least as long as the resulting `Hasher` instance
    ///
    /// # Returns
    ///
    /// A new `Hasher` instance configured for MD5 hashing with the 16-byte hash size
    ///
    /// # Errors
    ///
    /// This constructor itself cannot fail, but the resulting `Hasher` may produce
    /// errors during hash computation if:
    /// * The target file cannot be opened or read
    /// * File contents cannot be properly hashed
    /// * Hash results cannot be converted to UTF-8 strings
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// let hasher = Hasher::new_md5(Path::new("file.txt"));
    /// ```
    #[must_use]
    pub fn new_md5(path: &'a Path) -> Hasher<'a, MD5_SIZE> {
        debug!("Hashing with MD5...");
        let algo = HashingAlgo::Md5;
        let hasher = Hasher::<'a, MD5_SIZE>::new(path, algo);
        hasher
    }
}

impl<'a> Hasher<'a, SHA_SIZE> {
    /// Creates a new `Hasher` instance configured to use the SHA256 hashing algorithm.
    ///
    /// This is a convenience constructor that configures the hasher with the SHA256 algorithm,
    /// which is slower but cryptographically stronger than MD5. The hasher will process
    /// the target file in 1MB chunks to build a Merkle tree of SHA256 hashes.
    ///
    /// SHA256 produces 32-byte (256-bit) hashes and is suitable for file integrity verification
    /// in security-critical contexts where cryptographic security is prioritized over performance.
    /// For better performance in non-security-critical applications, consider using [`Hasher::new_md5()`].
    ///
    /// This method enforces the correct hash size using Rust's const generics system,
    /// making it impossible to create a SHA256 hasher with the wrong output size.
    ///
    /// # Arguments
    ///
    /// * `path` - A reference to the path of the file to be hashed, which must live
    ///   at least as long as the resulting `Hasher` instance
    ///
    /// # Returns
    ///
    /// A new `Hasher` instance configured for SHA256 hashing with the 32-byte hash size
    ///
    /// # Errors
    ///
    /// This constructor itself cannot fail, but the resulting `Hasher` may produce
    /// errors during hash computation if:
    /// * The target file cannot be opened or read
    /// * File contents cannot be properly hashed
    /// * Hash results cannot be converted to UTF-8 strings
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// let hasher = Hasher::new_sha2(Path::new("file.txt"));
    /// ```
    #[must_use]
    pub fn new_sha2(path: &'a Path) -> Hasher<'a, SHA_SIZE> {
        let algo = HashingAlgo::Sha2;
        let hasher = Hasher::<'a, SHA_SIZE>::new(path, algo);
        hasher
    }
}

impl<'a, const N: usize> Hasher<'a, N> {
    #[must_use]
    pub fn new(path: &'a Path, algorithm: HashingAlgo) -> Hasher<'a, N> {
        Hasher { path, algorithm }
    }

    /// Computes the initial set of hashes for a file using the chosen hashing algorithm.
    ///
    /// This method reads the target file in 1MB chunks and computes individual hashes for each chunk,
    /// forming the base layer of a Merkle tree. These hashes are stored in a vector and wrapped in
    /// a `HashArray` struct that implements the `MerkleIter` trait, enabling parallel computation
    /// of the Merkle tree's higher layers.
    ///
    /// The method is generic over any digest implementation that satisfies the `Digest + Default` traits,
    /// but is typically used with either MD5 or SHA256 through the `compute_starter_hashes::<Md5>()` or
    /// `compute_starter_hashes::<Sha256>()` specializations.
    ///
    /// # Type Parameters
    ///
    /// * `D` - The digest implementation to use, must implement `Digest + Default`
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a type that implements `MerkleIter<N>`, where `N` is the hash size
    /// in bytes (16 for MD5, 32 for SHA256). The returned iterator contains the base layer hashes of
    /// the Merkle tree.
    ///
    /// # Errors
    ///
    /// This method will return an error if:
    /// * The target file cannot be opened for reading
    /// * There are I/O errors while reading the file chunks
    /// * The hash output size doesn't match the expected size `N`
    /// * The hash results cannot be properly converted to fixed-size arrays
    ///
    /// # Examples
    ///
    /// This method is typically used internally as part of the Merkle tree computation process,
    /// but can be called directly to access the raw chunk hashes of a file.
    #[allow(clippy::large_stack_arrays)]
    pub fn compute_starter_hashes<D: Digest + Default>(&self) -> Result<impl MerkleIter<N>> {
        let mut hashes: Vec<[u8; N]> = Vec::new();

        let file = File::open(self.path)?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; CHUNK_SIZE];

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            let mut default_hasher = D::default();
            default_hasher.update(&buffer[..bytes_read]);
            let hash_result: [u8; N] = default_hasher.finalize().as_ref().try_into()?;
            hashes.push(hash_result);
        }

        let hash_array = HashArray { hashes };
        Ok(hash_array)
    }

    /// Recursively computes the root hash of a Merkle tree for a file.
    ///
    /// This method coordinates the full process of computing a file's Merkle root hash by:
    /// 1. Computing initial hashes for each 1MB chunk of the file
    /// 2. Building the complete Merkle tree by recursively combining pairs of hashes
    /// 3. Converting the final root hash into a UTF-8 string
    ///
    /// The method dynamically dispatches between MD5 and SHA256 based on the `algorithm` field,
    /// using parallel processing via rayon for better performance. The final root hash is
    /// returned as a String to allow for easy storage and comparison.
    ///
    /// Each level of the Merkle tree is computed by taking pairs of hashes from the previous
    /// level, concatenating them, and hashing the result. Unpaired hashes at the end of a
    /// level are carried forward unchanged.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the UTF-8 string representation of the root hash if
    /// successful.
    ///
    /// # Errors
    ///
    /// This method will return an error in the following situations:
    /// * If reading the file fails
    /// * If hashing operations fail
    /// * If converting the final hash to a UTF-8 string fails
    /// * If intermediate hashes cannot be properly combined
    ///
    /// # Panics
    ///
    /// This method may panic if:
    /// * The hash size specified by N doesn't match the output size of the chosen digest
    /// * The buffer size for file chunks is not properly aligned
    /// * Internal array conversions fail due to size mismatches
    ///
    /// # Examples
    ///
    /// This is typically used as part of file verification through the `checksum()` method,
    /// but can also be used directly to compute a file's root hash:
    ///
    /// ```no_run
    /// use checkle::*;
    /// use std::path::Path;
    /// let hasher = Hasher::new_md5(Path::new("file.txt"));
    /// let root_hash = hasher.find_root_hash().unwrap();
    /// ```
    ///
    /// # Implementation Notes
    ///
    /// The method uses dynamic dispatch to handle both MD5 (16-byte) and SHA256 (32-byte)
    /// hashes, abstracting over the different hash sizes by converting fixed arrays to
    /// heap-allocated vectors for the final result.
    ///
    pub fn find_root_hash(self) -> Result<String> {
        // run all hashes in the merkle tree and collect them into a Vec of heap-allocated Vec's,
        // which allows us to abstract over the different hash sizes across algorithms
        let root_hash_vec: Vec<Vec<u8>> = match self.algorithm {
            // Hash with Md5, processing each chunk in parallele with a rayon iterator
            HashingAlgo::Md5 => self
                .compute_starter_hashes::<Md5>()?
                .par_iter_merkle::<Md5>()?
                .get_hashes()
                .into_iter()
                .map(|hash: [u8; N]| hash.to_vec())
                .collect(),

            // Do the same but with the slower but more secure SHA256 algo
            HashingAlgo::Sha2 => self
                .compute_starter_hashes::<Sha256>()?
                .par_iter_merkle::<Sha256>()?
                .get_hashes()
                .into_iter()
                .map(|hash: [u8; N]| hash.to_vec())
                .collect(),
        };
        assert_eq!(root_hash_vec.len(), 1);

        // Convert the binary hash to a hexadecimal string
        let hex_hash = root_hash_vec[0]
            .iter()
            .fold(String::new(), |mut acc, byte| {
                use std::fmt::Write;
                let _ = write!(acc, "{byte:02x}");
                acc
            })
            .to_string();

        debug!("Hashing of {:?} was successful.", self.path);

        Ok(hex_hash)
    }

    /// Verifies a file's integrity by comparing its current Merkle root hash with a previously stored hash.
    ///
    /// This method computes the current Merkle tree root hash of a file and compares it with a provided
    /// hash value, typically one that was stored when the file was in a known-good state. The comparison
    /// helps detect any changes or corruption in the file's contents.
    ///
    /// The method uses the same hashing algorithm (MD5 or SHA256) that was used to generate the original
    /// hash. For security-critical applications, SHA256 is recommended over MD5.
    ///
    /// This method is a key part of the verification workflow in checkle, used internally by
    /// [`FilesToCheck::checksum_all()`] when verifying multiple files. It provides a simpler interface
    /// compared to manually calling [`find_root_hash()`] and comparing the results.
    ///
    /// # Arguments
    ///
    /// * `self` - The hasher instance, which includes the file path and selected algorithm
    /// * `old_hash` - The previously stored hash value to compare against
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the hashes match, indicating the file is unchanged.
    /// Returns an error if the hashes don't match or if hash computation fails.
    ///
    /// # Errors
    ///
    /// This method will return an error in the following situations:
    /// * If computing the new hash fails (e.g., due to file I/O errors)
    /// * If the computed hash doesn't match the provided hash (indicates file modification)
    /// * If there are encoding issues when converting hashes to strings
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    /// let hasher = Hasher::new_md5(Path::new("file.txt"));
    /// let stored_hash = "previously_stored_hash_value";
    /// match hasher.checksum(stored_hash) {
    ///     Ok(()) => println!("File integrity verified"),
    ///     Err(_) => println!("File has been modified"),
    /// }
    /// ```
    ///
    /// # See Also
    ///
    /// * [`find_root_hash()`] - For computing just the root hash without comparison
    /// * [`FilesToCheck::checksum_all()`] - For verifying multiple files at once
    pub fn checksum(self, old_hash: &str) -> Result<()> {
        let file = self.path.to_path_buf();
        let new_hash = self.find_root_hash()?;
        compared_hashes(old_hash, &new_hash, &file)?;

        Ok(())
    }
}

/// A simple array struct that contains a vector of fixed-size hashes used in Merkle tree computation.
///
/// This struct is used internally to store and process hashes when building Merkle trees for file
/// integrity verification. It works with both MD5 (16-byte) and SHA256 (32-byte) hashes through
/// the const generic parameter N.
///
/// The struct implements the `MerkleIter` trait which enables parallel processing of hash pairs
/// to efficiently compute successive layers of the Merkle tree until reaching the root hash.
/// Together with the `Hasher` struct, it forms the core mechanism for file checksumming in checkle.
///
/// The hashes are stored as fixed-size byte arrays to ensure proper sizing based on the chosen
/// algorithm, while the Vec provides dynamic sizing as needed when processing variable-sized files.
pub struct HashArray<const N: usize> {
    hashes: Vec<[u8; N]>,
}

/// A trait that defines iteration behavior for Merkle tree construction and hash computation.
///
/// This trait is fundamental to checkle's file integrity verification system, providing methods
/// to iteratively compute Merkle tree hashes in parallel. It works with both MD5 (16-byte) and
/// SHA256 (32-byte) hashes through const generic parameters.
///
/// The trait is primarily implemented by [`HashArray`], which stores vectors of fixed-size hash arrays
/// and provides the core functionality for building Merkle trees layer by layer until reaching a root hash.
///
/// # Type Parameters
///
/// * `N` - A const generic parameter representing the hash size in bytes (16 for MD5, 32 for SHA256)
///
/// # Required Methods
///
/// Implementors must provide:
/// * `par_iter_merkle<D>` - Computes the next layer of Merkle tree hashes in parallel
/// * `get_hashes` - Returns the current layer's hashes
/// * `len` - Returns the number of hashes in the current layer
///
/// # Optional Methods
///
/// * `is_empty` - A default implementation is provided based on `len()`
///
/// # Implementation Notes
///
/// When implementing this trait:
/// * Merkle tree computation should process hash pairs in parallel where possible
/// * Unpaired hashes at layer boundaries should be carried forward unchanged
/// * Hash sizes must match the const generic parameter N
/// * The implementation should be compatible with both MD5 and SHA256 digests
///
/// This trait is essential for checkle's performance, as it enables parallel processing
/// of large files through rayon's parallel iterators while maintaining type safety through
/// const generics.
pub trait MerkleIter<const N: usize> {
    /// Computes the next layer of a Merkle tree by combining pairs of hashes in parallel.
    ///
    /// This method is a core building block for constructing Merkle trees, used internally by
    /// the [`Hasher`] to verify file integrity. It processes pairs of hashes from the current
    /// layer to produce parent hashes for the next layer up in the tree, continuing until a
    /// single root hash is reached.
    ///
    /// The implementation leverages rayon for parallel processing and works with both MD5 and
    /// SHA256 through the const generic parameter N and digest type D.
    ///
    /// # Arguments
    ///
    /// * `self` - The current layer's hash array to process
    ///
    /// # Returns
    ///
    /// Returns a Result containing a new `HashArray` with the next layer's hashes. When only one
    /// hash remains, it is the Merkle tree's root hash.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * Hash computation fails for any pair
    /// * The computed hash size doesn't match N
    /// * Hash results cannot be converted to fixed-size arrays
    ///
    /// # Panics
    ///
    /// This method may panic if the hash digest's output size does not match the expected
    /// size N specified by the const generic parameter.
    fn par_iter_merkle<D: Digest + Default>(self) -> Result<HashArray<N>>;
    fn get_hashes(self) -> Vec<[u8; N]>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool
    where
        Self: Sized,
    {
        self.len() == 0
    }
}

impl<const N: usize> MerkleIter<N> for HashArray<N> {
    /// Computes the next layer of a Merkle tree by combining adjacent pairs of hashes.
    ///
    /// This is a key implementation for efficient Merkle tree construction, using rayon's parallel
    /// iterators to process hash pairs concurrently. For each pair of hashes in the current layer,
    /// the method concatenates them and computes a new hash, forming the next layer up in the tree.
    /// If there's an unpaired hash at the end of a layer, it's carried forward unchanged.
    ///
    /// The implementation works with both MD5 (16-byte) and SHA256 (32-byte) hashes through the
    /// const generic parameter N and the generic digest type D. It recursively continues processing
    /// layers until reaching a single root hash, providing the foundation for checkle's file
    /// integrity verification system.
    ///
    /// # Type Parameters
    ///
    /// * `D` - The digest implementation to use, must implement `Digest + Default`
    /// * `N` - The hash size in bytes (16 for MD5, 32 for SHA256)
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a new `HashArray` with the computed hashes for the next
    /// Merkle tree layer. When the returned array contains only one hash, that hash is the
    /// root of the completed Merkle tree.
    ///
    /// # Errors
    ///
    /// This method returns an error if:
    /// * Hash computation fails for any pair
    /// * The computed hash size doesn't match N
    /// * Converting hash results to fixed-size arrays fails
    ///
    /// # Panics
    ///
    /// This method will panic if the hash digest's output cannot be converted to a fixed-size
    /// array of size N. This should never occur with properly matched hash algorithms and sizes.
    ///
    /// # Algorithm
    ///
    /// 1. If the current layer has only one hash, return it (base case)
    /// 2. Group hashes into pairs
    /// 3. For each pair:
    ///    - Concatenate the hashes
    ///    - Compute a new hash of the concatenated values
    ///    - For unpaired hashes, carry forward unchanged
    /// 4. Recursively process the new layer until reaching the root
    ///
    /// The parallel processing of hash pairs makes this implementation especially efficient
    /// for large files that produce many initial hashes.
    fn par_iter_merkle<D: Digest + Default>(self) -> Result<HashArray<N>> {
        if self.hashes.len() == 1 {
            return Ok(self);
        }
        let chunks = self.hashes.chunks(2).collect::<Vec<&[[u8; N]]>>();
        let current_hashes: Vec<[u8; N]> = chunks
            .into_par_iter()
            .map(|hash_pair| {
                let mut digest = D::default();
                if hash_pair.len() == 2 {
                    digest.update(hash_pair[0]);
                    digest.update(hash_pair[1]);
                } else {
                    digest.update(hash_pair[0]);
                }
                let updated_hash: [u8; N] = digest.finalize().as_ref().try_into().unwrap();
                updated_hash
            })
            .collect();

        let current_array = HashArray {
            hashes: current_hashes,
        };

        // recursively continue the search for the root hash
        let output_hashes = HashArray::par_iter_merkle::<D>(current_array)?;

        Ok(output_hashes)
    }

    fn get_hashes(self) -> Vec<[u8; N]> {
        self.hashes
    }

    fn len(&self) -> usize {
        self.hashes.len()
    }
}

impl FilesToCheck {
    /// Processes a collection of files in parallel to verify their integrity using Merkle tree checksums.
    ///
    /// This is the main entry point for batch file verification in checkle. It processes each file in
    /// parallel using rayon, computing a new Merkle tree hash and comparing it against a previously stored
    /// hash value. The method implements the core verification workflow of the crate, leveraging the
    /// [`Hasher`] and [`MerkleIter`] components to efficiently process multiple files.
    ///
    /// The method strikes a balance between performance and security by allowing selection between MD5
    /// (faster but less secure) and SHA256 (slower but cryptographically secure) hashing algorithms.
    /// For non-security-critical applications where performance is important, MD5 is recommended.
    /// For security-critical applications, SHA256 should be used.
    ///
    /// # Arguments
    ///
    /// * `self` - Consumes the `FilesToCheck` instance containing the files to verify
    /// * `algo` - The hashing algorithm to use, either [`HashingAlgo::Md5`] or [`HashingAlgo::Sha2`]
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all files pass their checksum verification.
    /// Returns a `CheckleError` variant if any files fail verification.
    ///
    /// # Errors
    ///
    /// This method can return several variants of `CheckleError`:
    /// * `FailedChecksum` - If exactly one file fails verification
    /// * `MultipleFailedChecksums` - If multiple files fail verification
    ///
    /// Additionally, errors may occur during hash computation if:
    /// * Files cannot be opened or read
    /// * Hash computation fails
    /// * Hash string conversion fails
    ///
    /// # Examples
    ///
    /// To verify multiple files using MD5 hashing:
    /// ```no_run
    /// let files = FilesToCheck::new(vec![/* file paths and hashes */]);
    /// files.checksum_all(&HashingAlgo::Md5)?;
    /// ```
    ///
    /// # Notes
    ///
    /// - Files are processed in parallel using rayon's parallel iterator
    /// - Progress information is logged using the `log` crate
    /// - The method consumes self to ensure clean ownership semantics
    /// - Failed verifications are collected and reported together
    pub fn checksum_all(self, algo: &HashingAlgo) -> Result<(), CheckleError> {
        let results = self
            .to_vec()
            .into_par_iter()
            .map(|pair| {
                let (file, old_hash) = pair.file_hash_owned();

                match algo {
                    HashingAlgo::Md5 => {
                        let hasher = Hasher::new_md5(&file);
                        let Ok(new_hash) = hasher.find_root_hash() else {
                            return Err(file);
                        };
                        if compared_hashes(&old_hash, &new_hash, &file).is_ok() {
                            info!("The file {:?} passed its checksum", &file);
                            Ok(())
                        } else {
                            Err(file)
                        }
                    }
                    HashingAlgo::Sha2 => {
                        let hasher = Hasher::new_sha2(&file);
                        let Ok(new_hash) = hasher.find_root_hash() else {
                            return Err(file);
                        };
                        if compared_hashes(&old_hash, &new_hash, &file).is_ok() {
                            info!("The file {:?} passed its checksum", &file);
                            Ok(())
                        } else {
                            Err(file)
                        }
                    }
                }
            })
            .map(|result| {
                if let Err(file) = result {
                    Some(file)
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        if results.is_empty() {
            return Ok(());
        }

        if results.len() == 1 {
            let failed_file = results[0].clone();
            return Err(FailedChecksum(failed_file));
        }

        let mut error_string = "The following files failed their checksums:".to_string();
        for file in results {
            error_string = format!("{error_string}\n{file:?}");
        }
        warn!("{error_string}");

        Err(MultipleFailedChecksums)
    }
}

/// A helper function that compares two hashes and returns an error if they don't match.
///
/// This function supports the core integrity verification process in checkle by comparing
/// previously stored hash values with newly computed ones. It's used internally by
/// [`Hasher::checksum()`] and [`FilesToCheck::checksum_all()`] to detect file modifications.
///
/// # Arguments
///
/// * `old_hash` - The previously stored hash value to compare against
/// * `new_hash` - The newly computed hash value
/// * `file` - The path of the file being verified, used for error reporting
///
/// # Returns
///
/// Returns `Ok(())` if the hashes match, indicating file integrity is preserved.
/// Returns `Err(FailedChecksum)` if hashes differ, indicating file modification.
///
/// # Errors
///
/// Returns a `FailedChecksum` error containing the file path if the hash values don't match.
///
#[inline]
pub fn compared_hashes(old_hash: &str, new_hash: &str, file: &Path) -> Result<(), CheckleError> {
    if old_hash == new_hash {
        Ok(())
    } else {
        Err(FailedChecksum(file.to_path_buf()))
    }
}
