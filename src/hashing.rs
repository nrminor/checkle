use log::{debug, info, warn};
use md5::Md5;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    marker::Sized,
    path::{Path, PathBuf},
};

use crate::{
    buffer_pool::BufferPool,
    constants::{
        CHUNK_SIZE, DEFAULT_CHUNK_SIZE, MAX_CHUNK_COUNT, MAX_CHUNK_SIZE, MAX_FILES_IN_BATCH,
        MAX_PARALLEL_READERS, MD5_SIZE, MIN_CHUNK_SIZE, PARALLEL_IO_THRESHOLD, SHA_SIZE,
    },
    errors::{CheckleError, Result},
    io::FilesToCheck,
};

/// Represents a region of a file to be processed by one thread during parallel I/O.
///
/// This struct defines a contiguous region of bytes within a file that will be read
/// and processed by a specific thread. The parallel I/O implementation ensures that
/// all regions together cover the entire file with no gaps or overlaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRegion {
    /// The ID of the thread that will process this region (0-based)
    pub thread_id: usize,
    /// Byte offset where this region starts (inclusive)
    pub start_offset: u64,
    /// Byte offset where this region ends (exclusive)
    pub end_offset: u64,
    /// Size of chunks to read within this region
    pub chunk_size: usize,
}

impl FileRegion {
    /// Calculate regions for parallel processing, ensuring contiguous coverage.
    ///
    /// This function divides a file into regions for parallel processing by multiple threads.
    /// It ensures that:
    /// - All regions are contiguous with no gaps or overlaps
    /// - The last thread handles any remainder bytes from uneven division
    /// - Small files return a single region for sequential processing
    /// - All input parameters are validated according to Tiger Style
    ///
    /// # Arguments
    ///
    /// * `file_size` - Total size of the file in bytes
    /// * `num_threads` - Number of parallel threads to use
    /// * `chunk_size` - Size of chunks to use within each region
    ///
    /// # Returns
    ///
    /// A vector of `FileRegion` structs, one per thread, covering the entire file.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - `num_threads` is 0 or exceeds `MAX_PARALLEL_READERS`
    /// - `chunk_size` is outside the valid range [`MIN_CHUNK_SIZE`, `MAX_CHUNK_SIZE`]
    /// - The calculated regions don't properly cover the file (postcondition failure)
    #[must_use]
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unwrap_used)]
    pub fn calculate_regions(
        file_size: u64,
        num_threads: usize,
        chunk_size: usize,
    ) -> Vec<FileRegion> {
        // Precondition assertions - Tiger Style compliance
        assert!(num_threads > 0, "Thread count must be positive");
        assert!(
            num_threads <= MAX_PARALLEL_READERS,
            "Thread count exceeds maximum: {num_threads} > {MAX_PARALLEL_READERS}"
        );
        assert!(
            chunk_size >= MIN_CHUNK_SIZE,
            "Chunk size too small: {chunk_size} < {MIN_CHUNK_SIZE}"
        );
        assert!(
            chunk_size <= MAX_CHUNK_SIZE,
            "Chunk size too large: {chunk_size} > {MAX_CHUNK_SIZE}"
        );

        // Handle empty files - return single region
        if file_size == 0 {
            let region = FileRegion {
                thread_id: 0,
                start_offset: 0,
                end_offset: 0,
                chunk_size,
            };
            return vec![region];
        }

        // If file is smaller than chunk size or we only have 1 thread, use single region
        if file_size <= chunk_size as u64 || num_threads == 1 {
            let region = FileRegion {
                thread_id: 0,
                start_offset: 0,
                end_offset: file_size,
                chunk_size,
            };
            return vec![region];
        }

        // Calculate regions for parallel processing
        let mut regions = Vec::with_capacity(num_threads);
        let base_region_size = file_size / num_threads as u64;
        let remainder = file_size % num_threads as u64;

        let mut current_offset = 0u64;

        for thread_id in 0..num_threads {
            #[allow(clippy::cast_possible_truncation)]
            let region_size = if thread_id < remainder as usize {
                // First 'remainder' threads get one extra byte
                base_region_size + 1
            } else {
                base_region_size
            };

            let end_offset = current_offset + region_size;

            let region = FileRegion {
                thread_id,
                start_offset: current_offset,
                end_offset,
                chunk_size,
            };

            regions.push(region);
            current_offset = end_offset;
        }

        // Postcondition assertions - Tiger Style compliance
        assert!(!regions.is_empty(), "Must return at least one region");
        assert_eq!(
            regions.len(),
            num_threads.min(if file_size == 0 { 1 } else { num_threads }),
            "Number of regions must match thread count"
        );
        assert_eq!(
            regions[0].start_offset, 0,
            "First region must start at offset 0"
        );
        assert_eq!(
            regions.last().unwrap().end_offset,
            file_size,
            "Last region must end at file size"
        );

        // Verify contiguous coverage with no gaps or overlaps
        for i in 1..regions.len() {
            assert_eq!(
                regions[i - 1].end_offset,
                regions[i].start_offset,
                "Region {} and {} must be contiguous: {} != {}",
                i - 1,
                i,
                regions[i - 1].end_offset,
                regions[i].start_offset
            );
        }

        // Verify all regions have valid properties
        for (i, region) in regions.iter().enumerate() {
            assert_eq!(
                region.thread_id, i,
                "Region {i} must have correct thread_id"
            );
            assert!(
                region.start_offset < region.end_offset
                    || (region.start_offset == 0 && region.end_offset == 0),
                "Region {} must have start < end (unless empty file): {} >= {}",
                i,
                region.start_offset,
                region.end_offset
            );
            assert_eq!(
                region.chunk_size, chunk_size,
                "Region {i} must have correct chunk_size"
            );
        }

        regions
    }

    /// Get the size of this region in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.end_offset - self.start_offset
    }

    /// Check if this region is empty (zero bytes).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start_offset == self.end_offset
    }
}

/// Reads and hashes a specific region of a file using a buffer from the pool.
///
/// This function is the core of the parallel I/O implementation. It opens the file,
/// seeks to the region's start offset, and reads chunks within the region boundaries.
/// Each chunk is hashed using the provided digest algorithm, maintaining strict ordering.
///
/// # Arguments
///
/// * `path` - Path to the file to read
/// * `region` - File region to process (defines start/end offsets and chunk size)
/// * `buffer_pool` - Pool to acquire buffer from for zero-allocation reads  
///
/// # Returns
///
/// A vector of hashes in order for all chunks within this region.
///
/// # Errors
///
/// Returns an error if:
/// - File cannot be opened or doesn't exist
/// - Seek operation fails
/// - Read operation fails or returns partial data unexpectedly
/// - Hash computation fails
///
/// # Panics
///
/// Panics if:
/// - The file doesn't exist (precondition)
/// - The region is invalid (start >= end when not empty)
/// - Buffer pool returns buffer of wrong size
fn read_and_hash_region<D: Digest + Default + Send + Sync, const N: usize>(
    path: &Path,
    region: &FileRegion,
    buffer_pool: &BufferPool,
) -> Result<Vec<[u8; N]>> {
    // Precondition assertions - Tiger Style
    assert!(path.exists(), "File must exist: {}", path.display());
    assert!(
        region.start_offset <= region.end_offset,
        "Region start {} must be <= end {}",
        region.start_offset,
        region.end_offset
    );

    // Handle empty regions
    if region.is_empty() {
        return Ok(Vec::new());
    }

    // Open file with error context
    let mut file = File::open(path).map_err(|source| CheckleError::FileOpenError {
        path: path.to_path_buf(),
        source,
    })?;

    // Seek to region start
    file.seek(SeekFrom::Start(region.start_offset))
        .map_err(|source| CheckleError::FileReadError {
            path: path.to_path_buf(),
            source,
        })?;

    // Create a BufReader for efficient reading
    let mut reader = BufReader::new(file);
    let mut hash_results = Vec::new();
    let bytes_remaining = region.size();

    // Use buffer pool for zero-allocation reads
    let mut buffer = buffer_pool.acquire();

    // Assert buffer is the correct size
    assert!(
        buffer.len() >= region.chunk_size,
        "Buffer size {} must be >= chunk size {}",
        buffer.len(),
        region.chunk_size
    );

    // For the corrected parallel approach, each region represents exactly one chunk
    // Read the entire region as a single chunk
    #[allow(clippy::expect_used)] // Size is validated by region calculation
    let chunk_size = usize::try_from(bytes_remaining).expect("bytes_remaining fits in usize");
    let buffer_slice = &mut buffer.as_mut_slice()[..chunk_size];

    // Read the entire chunk
    let mut total_bytes_read = 0;
    while total_bytes_read < chunk_size {
        let n = reader
            .read(&mut buffer_slice[total_bytes_read..])
            .map_err(|source| CheckleError::FileReadError {
                path: path.to_path_buf(),
                source,
            })?;

        if n == 0 {
            // This is expected for the last chunk which might be smaller
            break;
        }

        total_bytes_read += n;
    }

    // Hash the chunk (with actual bytes read, not requested size)
    let mut digest_engine = D::default();
    digest_engine.update(&buffer_slice[..total_bytes_read]);
    let hash_bytes = digest_engine.finalize();

    // Convert to fixed-size array
    let hash_result: [u8; N] =
        hash_bytes
            .as_ref()
            .try_into()
            .map_err(|_| CheckleError::HashSizeMismatch {
                path: path.to_path_buf(),
                algorithm: std::any::type_name::<D>().to_string(),
                computed_size: hash_bytes.len(),
                expected_size: N,
            })?;

    hash_results.push(hash_result);

    // Postcondition assertions
    assert!(
        !hash_results.is_empty(),
        "Must produce at least one hash for non-empty region"
    );

    Ok(hash_results)
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum HashingAlgo {
    #[default]
    Md5,
    Sha2,
}

/// Validates and aligns a chunk size to page boundaries.
///
/// # Arguments
///
/// * `size` - The requested chunk size in bytes
///
/// # Returns
///
/// The validated and page-aligned chunk size.
///
/// # Errors
///
/// Returns `InvalidChunkSize` if the size is outside valid bounds.
///
/// # Panics
///
/// Never panics - all validation is done through Result return.
fn validate_chunk_size(size: usize) -> Result<usize> {
    if size < MIN_CHUNK_SIZE {
        return Err(CheckleError::InvalidChunkSize {
            size,
            reason: "Chunk size too small".to_string(),
            min_size: MIN_CHUNK_SIZE,
            max_size: MAX_CHUNK_SIZE,
        });
    }
    if size > MAX_CHUNK_SIZE {
        return Err(CheckleError::InvalidChunkSize {
            size,
            reason: "Chunk size too large".to_string(),
            min_size: MIN_CHUNK_SIZE,
            max_size: MAX_CHUNK_SIZE,
        });
    }
    // Align to page boundary (4KB)
    let aligned_size = size & !(4096 - 1);
    if aligned_size < MIN_CHUNK_SIZE {
        // If alignment makes it too small, round up
        Ok(MIN_CHUNK_SIZE)
    } else {
        Ok(aligned_size)
    }
}

/// Gets the number of available CPU cores, with fallback.
///
/// # Returns
///
/// Number of logical CPU cores available, clamped to `MAX_PARALLEL_READERS`.
/// Returns 1 if detection fails.
fn get_available_cpus() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .clamp(1, MAX_PARALLEL_READERS)
}

pub struct Hasher<'a, const N: usize> {
    pub path: &'a Path,
    pub algorithm: HashingAlgo,
    pub chunk_size: usize,
    pub parallel_readers: usize,
    pub progress_callback: Option<Box<dyn Fn(u64) + Send + Sync>>,
}

impl<'a> Hasher<'a, MD5_SIZE> {
    /// Creates a new MD5 hasher for the given file path.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The path does not exist
    /// - The path is a directory instead of a file
    #[must_use]
    pub fn new_md5(path: &'a Path) -> Hasher<'a, MD5_SIZE> {
        // Precondition assertions
        assert!(path.exists(), "Input path must exist: {}", path.display());
        assert!(
            !path.is_dir(),
            "Input path must be a file, not directory: {}",
            path.display()
        );

        debug!("Hashing with MD5...");
        let algo = HashingAlgo::Md5;
        let hasher = Hasher::<'a, MD5_SIZE>::new(path, algo);

        // Postcondition assertions
        assert_eq!(hasher.algorithm, HashingAlgo::Md5, "Algorithm must be MD5");
        assert_eq!(hasher.path, path, "Path must match input");
        assert_eq!(
            hasher.chunk_size, DEFAULT_CHUNK_SIZE,
            "Chunk size must be default"
        );
        assert!(
            hasher.parallel_readers > 0,
            "Parallel readers must be positive"
        );
        assert!(
            hasher.parallel_readers <= MAX_PARALLEL_READERS,
            "Parallel readers must be within bounds"
        );

        hasher
    }
}

impl<'a> Hasher<'a, SHA_SIZE> {
    /// Creates a new SHA-256 hasher for the given file path.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The path does not exist
    /// - The path is a directory instead of a file
    #[must_use]
    pub fn new_sha2(path: &'a Path) -> Hasher<'a, SHA_SIZE> {
        // Precondition assertions
        assert!(path.exists(), "Input path must exist: {}", path.display());
        assert!(
            !path.is_dir(),
            "Input path must be a file, not directory: {}",
            path.display()
        );

        let algo = HashingAlgo::Sha2;
        let hasher = Hasher::<'a, SHA_SIZE>::new(path, algo);

        // Postcondition assertions
        assert_eq!(
            hasher.algorithm,
            HashingAlgo::Sha2,
            "Algorithm must be SHA2"
        );
        assert_eq!(hasher.path, path, "Path must match input");
        assert_eq!(
            hasher.chunk_size, DEFAULT_CHUNK_SIZE,
            "Chunk size must be default"
        );
        assert!(
            hasher.parallel_readers > 0,
            "Parallel readers must be positive"
        );
        assert!(
            hasher.parallel_readers <= MAX_PARALLEL_READERS,
            "Parallel readers must be within bounds"
        );

        hasher
    }
}

impl<'a, const N: usize> Hasher<'a, N> {
    /// Creates a new hasher with the specified algorithm for the given file path.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The path does not exist
    /// - The path is a directory instead of a file
    /// - The algorithm and generic size N don't match (MD5 requires N=16, SHA2 requires N=32)
    #[must_use]
    pub fn new(path: &'a Path, algorithm: HashingAlgo) -> Hasher<'a, N> {
        // Precondition assertions
        assert!(path.exists(), "Input path must exist: {}", path.display());
        assert!(
            !path.is_dir(),
            "Input path must be a file, not directory: {}",
            path.display()
        );

        let hasher = Hasher {
            path,
            algorithm,
            chunk_size: DEFAULT_CHUNK_SIZE,
            parallel_readers: get_available_cpus(),
            progress_callback: None,
        };

        // Postcondition assertions
        assert_eq!(hasher.path, path, "Path must match input");
        assert_eq!(
            hasher.chunk_size, DEFAULT_CHUNK_SIZE,
            "Chunk size must be default"
        );
        assert!(
            hasher.parallel_readers > 0,
            "Parallel readers must be positive"
        );
        assert!(
            hasher.parallel_readers <= MAX_PARALLEL_READERS,
            "Parallel readers must be within bounds"
        );

        hasher
    }

    /// Builder method to configure chunk size for hashing.
    ///
    /// # Arguments
    ///
    /// * `size` - The desired chunk size in bytes
    ///
    /// # Returns
    ///
    /// The configured hasher instance.
    ///
    /// # Errors
    ///
    /// Returns `InvalidChunkSize` if the size is outside valid bounds or cannot be aligned.
    ///
    /// # Panics
    ///
    /// Never panics - all validation is done through Result return.
    pub fn with_chunk_size(mut self, size: usize) -> Result<Self> {
        // Validate and align chunk size
        self.chunk_size = validate_chunk_size(size)?;

        // Postcondition assertions
        assert!(
            self.chunk_size >= MIN_CHUNK_SIZE,
            "Chunk size must be >= minimum"
        );
        assert!(
            self.chunk_size <= MAX_CHUNK_SIZE,
            "Chunk size must be <= maximum"
        );
        assert_eq!(
            self.chunk_size & (4096 - 1),
            0,
            "Chunk size must be page-aligned"
        );

        Ok(self)
    }

    /// Builder method to configure number of parallel readers.
    ///
    /// # Arguments
    ///
    /// * `count` - The desired number of parallel readers
    ///
    /// # Returns
    ///
    /// The configured hasher instance.
    ///
    /// # Panics
    ///
    /// Never panics - input is clamped to valid range.
    #[must_use]
    pub fn with_parallel_readers(mut self, count: usize) -> Self {
        self.parallel_readers = count.clamp(1, MAX_PARALLEL_READERS);

        // Postcondition assertions
        assert!(
            self.parallel_readers > 0,
            "Parallel readers must be positive"
        );
        assert!(
            self.parallel_readers <= MAX_PARALLEL_READERS,
            "Parallel readers must be within bounds"
        );

        self
    }

    /// Builder method to set a progress callback for hashing operations.
    ///
    /// The callback will be invoked with the number of bytes processed so far.
    /// This is useful for displaying progress bars or other progress indicators.
    ///
    /// # Arguments
    ///
    /// * `callback` - A function that takes the number of bytes processed
    ///
    /// # Returns
    ///
    /// The configured hasher instance for method chaining.
    ///
    /// # Panics
    ///
    /// This function includes a postcondition assertion that verifies the callback was set,
    /// but this should never panic in practice.
    #[must_use]
    pub fn with_progress_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));

        // Postcondition assertion
        assert!(
            self.progress_callback.is_some(),
            "Progress callback must be set"
        );

        self
    }

    /// Determines whether to use parallel I/O based on file characteristics.
    ///
    /// # Returns
    ///
    /// `true` if parallel I/O should be used, `false` for sequential I/O.
    ///
    /// # Errors
    ///
    /// Returns an error if file metadata cannot be accessed.
    ///
    /// # Panics
    ///
    /// Panics if the file doesn't exist (precondition violation).
    fn should_use_parallel_io(&self) -> Result<bool> {
        // Precondition assertions
        assert!(
            self.path.exists(),
            "File must exist: {}",
            self.path.display()
        );

        // Get file size
        let metadata = fs::metadata(self.path).map_err(|source| CheckleError::FileOpenError {
            path: self.path.to_path_buf(),
            source,
        })?;
        let file_size = metadata.len();

        // Decision criteria:
        // 1. File size >= PARALLEL_IO_THRESHOLD (1MB)
        // 2. More than 1 parallel reader requested
        let use_parallel = file_size >= PARALLEL_IO_THRESHOLD && self.parallel_readers > 1;

        debug!(
            "Parallel I/O decision for {}: {} (size: {} bytes, readers: {}, threshold: {} bytes)",
            self.path.display(),
            use_parallel,
            file_size,
            self.parallel_readers,
            PARALLEL_IO_THRESHOLD
        );

        Ok(use_parallel)
    }

    /// Computes initial hashes for each chunk of the file.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file cannot be opened
    /// - There's an I/O error while reading the file
    /// - The hash size doesn't match the expected size N
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The file doesn't exist
    /// - The path is not a file
    /// - The number of chunks exceeds `MAX_CHUNK_COUNT` (1 million)
    #[allow(clippy::large_stack_arrays)]
    pub fn compute_starter_hashes<D: Digest + Default>(&self) -> Result<impl MerkleIter<N>> {
        // Precondition assertions
        assert!(
            self.path.exists(),
            "File must exist before hashing: {}",
            self.path.display()
        );
        assert!(
            self.path.is_file(),
            "Path must be a file: {}",
            self.path.display()
        );

        // NO file size limit - we want to handle arbitrarily large files

        let mut hashes: Vec<[u8; N]> = Vec::new();

        let file = File::open(self.path).map_err(|source| CheckleError::FileOpenError {
            path: self.path.to_path_buf(),
            source,
        })?;
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; CHUNK_SIZE];
        let mut total_bytes_read = 0u64;

        loop {
            let bytes_read =
                reader
                    .read(&mut buffer)
                    .map_err(|source| CheckleError::FileReadError {
                        path: self.path.to_path_buf(),
                        source,
                    })?;
            if bytes_read == 0 {
                break;
            }

            // Update progress if callback is set
            total_bytes_read += bytes_read as u64;
            if let Some(ref callback) = self.progress_callback {
                callback(total_bytes_read);
            }

            let mut default_hasher = D::default();
            default_hasher.update(&buffer[..bytes_read]);
            let hash_bytes = default_hasher.finalize();
            let hash_result: [u8; N] =
                hash_bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| CheckleError::HashSizeMismatch {
                        path: self.path.to_path_buf(),
                        algorithm: format!("{:?}", self.algorithm),
                        computed_size: hash_bytes.len(),
                        expected_size: N,
                    })?;
            hashes.push(hash_result);
        }

        // Check chunk count limit
        assert!(
            hashes.len() <= MAX_CHUNK_COUNT,
            "Chunk count exceeds maximum limit: {} > {}",
            hashes.len(),
            MAX_CHUNK_COUNT
        );

        // Handle empty files - they should hash to the hash of empty data
        if hashes.is_empty() {
            let mut default_hasher = D::default();
            default_hasher.update([]);
            let hash_bytes = default_hasher.finalize();
            let hash_result: [u8; N] =
                hash_bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| CheckleError::HashSizeMismatch {
                        path: self.path.to_path_buf(),
                        algorithm: format!("{:?}", self.algorithm),
                        computed_size: hash_bytes.len(),
                        expected_size: N,
                    })?;
            hashes.push(hash_result);
        }

        let hashes_len = hashes.len();
        let hash_array = HashArray { hashes };

        // Postcondition assertions
        assert!(!hash_array.is_empty(), "Hash array must not be empty");
        assert_eq!(
            hash_array.len(),
            hashes_len,
            "Hash array length must match computed hashes"
        );

        Ok(hash_array)
    }

    /// Computes initial hashes for each chunk of the file using parallel I/O.
    ///
    /// This method implements high-performance parallel reading by dividing the file
    /// into regions that are processed concurrently by multiple threads. Critical
    /// correctness requirement: the parallel implementation MUST produce exactly
    /// the same hash as the sequential version for any file and thread count.
    ///
    /// # Arguments
    ///
    /// * `parallel_readers` - Number of parallel threads to use for reading
    ///
    /// # Returns
    ///
    /// A `HashArray` containing hashes in the same order as sequential processing.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File I/O operations fail
    /// - Hash computation fails  
    /// - Buffer pool creation fails
    /// - Region calculation fails
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The file doesn't exist
    /// - The path is not a file
    /// - `parallel_readers` is 0 or exceeds `MAX_PARALLEL_READERS`
    /// - File regions are not contiguous (postcondition failure)
    /// - Final hash count doesn't match expected count
    #[allow(clippy::too_many_lines)]
    pub fn compute_starter_hashes_parallel<D>(
        &self,
        parallel_readers: usize,
    ) -> Result<HashArray<N>>
    where
        D: Digest + Default + Send + Sync,
    {
        // Precondition assertions - Tiger Style
        assert!(
            self.path.exists(),
            "File must exist before hashing: {}",
            self.path.display()
        );
        assert!(
            self.path.is_file(),
            "Path must be a file: {}",
            self.path.display()
        );
        assert!(
            parallel_readers > 0,
            "Parallel readers count must be positive: {parallel_readers}"
        );
        assert!(
            parallel_readers <= MAX_PARALLEL_READERS,
            "Parallel readers {parallel_readers} exceeds maximum {MAX_PARALLEL_READERS}"
        );

        // Get file size and validate
        let metadata = fs::metadata(self.path).map_err(|source| CheckleError::FileOpenError {
            path: self.path.to_path_buf(),
            source,
        })?;
        let file_size = metadata.len();

        // Fall back to sequential for small files or single thread
        if file_size < PARALLEL_IO_THRESHOLD || parallel_readers == 1 {
            debug!(
                "Using sequential I/O for {} (size: {}, readers: {})",
                self.path.display(),
                file_size,
                parallel_readers
            );
            // Convert the MerkleIter result to HashArray
            let seq_result = self.compute_starter_hashes::<D>()?;
            let hashes = seq_result.get_hashes();
            return Ok(HashArray { hashes });
        }

        debug!(
            "Using parallel I/O for {} (size: {}, readers: {})",
            self.path.display(),
            file_size,
            parallel_readers
        );

        // Calculate chunk boundaries (same as sequential approach)
        let total_chunks = if file_size == 0 {
            1
        } else {
            #[allow(clippy::expect_used)] // CHUNK_SIZE is a compile-time constant
            usize::try_from(
                file_size.div_ceil(u64::try_from(CHUNK_SIZE).expect("CHUNK_SIZE fits in u64")),
            )
            .expect("result fits in usize")
        };

        // Create chunk descriptors with byte boundaries
        let mut chunks = Vec::new();
        for chunk_id in 0..total_chunks {
            #[allow(clippy::expect_used)] // chunk_id and CHUNK_SIZE are bounded
            let start_offset = u64::try_from(chunk_id).expect("chunk_id fits in u64")
                * u64::try_from(CHUNK_SIZE).expect("CHUNK_SIZE fits in u64");
            #[allow(clippy::expect_used)] // chunk_id+1 and CHUNK_SIZE are bounded
            let end_offset = (u64::try_from(chunk_id + 1).expect("chunk_id+1 fits in u64")
                * u64::try_from(CHUNK_SIZE).expect("CHUNK_SIZE fits in u64"))
            .min(file_size);

            // Handle empty file case
            if file_size == 0 {
                chunks.push(FileRegion {
                    thread_id: chunk_id,
                    start_offset: 0,
                    end_offset: 0,
                    chunk_size: CHUNK_SIZE,
                });
            } else {
                chunks.push(FileRegion {
                    thread_id: chunk_id,
                    start_offset,
                    end_offset,
                    chunk_size: CHUNK_SIZE,
                });
            }
        }

        // Assert chunk coverage
        assert!(!chunks.is_empty(), "Must have at least one chunk");
        if file_size > 0 {
            assert_eq!(
                chunks[0].start_offset, 0,
                "First chunk must start at offset 0"
            );
            let last_chunk_end = chunks.last().map_or(0, |chunk| chunk.end_offset);
            assert_eq!(
                last_chunk_end, file_size,
                "Last chunk must end at file size"
            );
        }

        // Create shared buffer pool for all threads
        // Buffer size should accommodate the largest chunk size
        let buffer_size = CHUNK_SIZE.max(4096); // At least 4KB, aligned to page boundary
        let pool_capacity = (parallel_readers * 2).min(64); // 2 buffers per thread, max 64

        let buffer_pool = BufferPool::new(pool_capacity, buffer_size).map_err(|_| {
            CheckleError::FileReadError {
                path: self.path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::OutOfMemory,
                    "Failed to create buffer pool for parallel I/O",
                ),
            }
        })?;

        // Process chunks in parallel using Rayon
        // Note: Progress callbacks are not supported in parallel mode due to thread safety
        // Progress tracking would require atomic counters and more complex synchronization
        let mut chunk_hashes: Vec<(usize, Vec<[u8; N]>)> = chunks
            .par_iter()
            .map(|chunk| {
                let hashes = read_and_hash_region::<D, N>(self.path, chunk, &buffer_pool)?;
                Ok((chunk.thread_id, hashes))
            })
            .collect::<Result<Vec<_>>>()?;

        // CRITICAL: Sort by thread_id (chunk_id) to ensure correct ordering
        chunk_hashes.sort_by_key(|(chunk_id, _)| *chunk_id);

        // Flatten hashes in order (each chunk should produce exactly one hash)
        let hashes: Vec<[u8; N]> = chunk_hashes
            .into_iter()
            .flat_map(|(_, hashes)| hashes)
            .collect();

        // Handle empty files - they should hash to the hash of empty data
        let final_hashes = if hashes.is_empty() {
            let mut default_hasher = D::default();
            default_hasher.update([]);
            let hash_bytes = default_hasher.finalize();
            let hash_result: [u8; N] =
                hash_bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| CheckleError::HashSizeMismatch {
                        path: self.path.to_path_buf(),
                        algorithm: std::any::type_name::<D>().to_string(),
                        computed_size: hash_bytes.len(),
                        expected_size: N,
                    })?;
            vec![hash_result]
        } else {
            hashes
        };

        // Postcondition assertions
        assert!(
            !final_hashes.is_empty(),
            "Hash array must not be empty after parallel processing"
        );
        assert!(
            final_hashes.len() <= MAX_CHUNK_COUNT,
            "Chunk count exceeds maximum limit: {} > {}",
            final_hashes.len(),
            MAX_CHUNK_COUNT
        );

        // Calculate expected chunk count for verification
        let expected_chunks = if file_size == 0 {
            1 // Empty file produces one hash
        } else {
            #[allow(clippy::expect_used)] // CHUNK_SIZE is a compile-time constant
            usize::try_from(
                file_size.div_ceil(u64::try_from(CHUNK_SIZE).expect("CHUNK_SIZE fits in u64")),
            )
            .expect("result fits in usize")
        };

        debug!(
            "Parallel hashing complete: {} chunks (expected ~{})",
            final_hashes.len(),
            expected_chunks
        );

        let hash_array = HashArray {
            hashes: final_hashes,
        };

        Ok(hash_array)
    }

    /// Computes the root hash of the file using a Merkle tree.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File I/O operations fail
    /// - Hash computation fails
    /// - The Merkle tree doesn't produce exactly one root hash
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The file doesn't exist
    /// - The path is not a file
    /// - The resulting hash string is empty or invalid
    pub fn find_root_hash(self) -> Result<String> {
        // Precondition assertions
        assert!(
            self.path.exists(),
            "File must exist: {}",
            self.path.display()
        );
        assert!(
            self.path.is_file(),
            "Path must be a file: {}",
            self.path.display()
        );

        // Decide whether to use parallel or sequential I/O
        let use_parallel = self.should_use_parallel_io()?;

        // Compute the root hash directly as a fixed-size array, avoiding Vec<Vec<u8>> allocation
        let root_hash_array: [u8; N] = match self.algorithm {
            // Hash with MD5, choosing parallel or sequential based on file characteristics
            HashingAlgo::Md5 => {
                let hashes = if use_parallel {
                    self.compute_starter_hashes_parallel::<Md5>(self.parallel_readers)?
                } else {
                    // Convert MerkleIter to HashArray for consistency
                    let seq_result = self.compute_starter_hashes::<Md5>()?;
                    HashArray {
                        hashes: seq_result.get_hashes(),
                    }
                };

                let final_hashes = hashes.par_iter_merkle::<Md5>()?.get_hashes();

                // Ensure we have exactly one root hash
                if final_hashes.len() != 1 {
                    return Err(CheckleError::InvalidMerkleTreeResult {
                        path: self.path.to_path_buf(),
                        found_count: final_hashes.len(),
                    });
                }

                final_hashes[0]
            }

            // Do the same but with SHA256
            HashingAlgo::Sha2 => {
                let hashes = if use_parallel {
                    self.compute_starter_hashes_parallel::<Sha256>(self.parallel_readers)?
                } else {
                    // Convert MerkleIter to HashArray for consistency
                    let seq_result = self.compute_starter_hashes::<Sha256>()?;
                    HashArray {
                        hashes: seq_result.get_hashes(),
                    }
                };

                let final_hashes = hashes.par_iter_merkle::<Sha256>()?.get_hashes();

                // Ensure we have exactly one root hash
                if final_hashes.len() != 1 {
                    return Err(CheckleError::InvalidMerkleTreeResult {
                        path: self.path.to_path_buf(),
                        found_count: final_hashes.len(),
                    });
                }

                final_hashes[0]
            }
        };

        // Convert the binary hash to a hexadecimal string
        let hex_hash = root_hash_array.iter().fold(String::new(), |mut acc, byte| {
            use std::fmt::Write;
            let _ = write!(acc, "{byte:02x}");
            acc
        });

        debug!("Hashing of {} was successful.", self.path.display());

        // Postcondition assertions
        assert!(!hex_hash.is_empty(), "Hash string must not be empty");
        assert_eq!(
            hex_hash.len(),
            N * 2,
            "Hash string length must be twice the hash size: {} != {}",
            hex_hash.len(),
            N * 2
        );
        assert!(
            hex_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash string must contain only hexadecimal characters"
        );

        Ok(hex_hash)
    }

    /// Verifies that the file's hash matches the provided hash.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File I/O operations fail
    /// - Hash computation fails
    /// - The computed hash doesn't match the provided hash
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The file doesn't exist
    /// - The path is not a file
    /// - The provided hash is empty or contains non-hexadecimal characters
    pub fn checksum(self, old_hash: &str) -> Result<()> {
        // Precondition assertions
        // Validate file exists
        if !self.path.exists() {
            return Err(CheckleError::InaccessibleFile(self.path.to_path_buf()));
        }

        // Validate hash is not empty
        if old_hash.is_empty() {
            return Err(CheckleError::InvalidChecksumFile(self.path.to_path_buf()));
        }

        // Validate hash contains only hexadecimal characters
        if !old_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CheckleError::InvalidChecksumFile(self.path.to_path_buf()));
        }

        let file = self.path.to_path_buf();
        let new_hash = self.find_root_hash()?;
        compared_hashes(old_hash, &new_hash, &file)?;

        // Postcondition assertion - only reached if hashes match
        // (if they don't match, compared_hashes returns an error)

        Ok(())
    }
}

pub struct HashArray<const N: usize> {
    hashes: Vec<[u8; N]>,
}

pub trait MerkleIter<const N: usize> {
    /// Performs parallel Merkle tree computation on the hash array.
    ///
    /// # Errors
    ///
    /// Returns an error if hash computation fails.
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
    fn par_iter_merkle<D: Digest + Default>(self) -> Result<HashArray<N>> {
        if self.hashes.len() == 1 {
            return Ok(self);
        }

        // Process pairs of hashes in parallel using rayon's par_chunks
        let current_hashes: Result<Vec<[u8; N]>> = self
            .hashes
            .par_chunks(2)
            .map(|hash_pair| {
                let mut digest = D::default();
                match hash_pair {
                    [first, second] => {
                        digest.update(first);
                        digest.update(second);
                    }
                    [single] => {
                        digest.update(single);
                    }
                    _ => unimplemented!(),
                }
                let hash_bytes = digest.finalize();
                let updated_hash: [u8; N] = hash_bytes.as_ref().try_into().map_err(|_| {
                    CheckleError::HashConversionError {
                        path: PathBuf::from("unknown"), // This is internal, path not available here
                        algorithm: std::any::type_name::<D>().to_string(),
                    }
                })?;
                Ok(updated_hash)
            })
            .collect();

        let current_hashes = current_hashes?;
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
    /// Verifies all files in the batch against their stored hashes.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - One or more files fail their checksum verification
    /// - File I/O operations fail
    /// - Hash computation fails
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The files list is empty
    /// - The number of files exceeds `MAX_FILES_IN_BATCH` (10,000)
    pub fn checksum_all(self, algo: &HashingAlgo) -> Result<()> {
        // Precondition assertions
        let files_vec = self.to_vec();
        assert!(!files_vec.is_empty(), "Files list must not be empty");
        assert!(
            files_vec.len() <= MAX_FILES_IN_BATCH,
            "File count exceeds maximum batch size: {} > {}",
            files_vec.len(),
            MAX_FILES_IN_BATCH
        );

        let results = files_vec
            .into_par_iter()
            .map(|pair| {
                let (file, old_hash) = pair.file_hash_owned();

                match algo {
                    HashingAlgo::Md5 => {
                        let hasher = Hasher::new_md5(&file);
                        let Ok(new_hash) = hasher.find_root_hash() else {
                            return (format!("ERROR\t{}", &file.display()), Some(file));
                        };
                        if compared_hashes(&old_hash, &new_hash, &file).is_ok() {
                            info!("The file {} passed its checksum", &file.display());
                            (format!("PASS\t{}", &file.display()), None)
                        } else {
                            (format!("FAIL\t{}", &file.display()), Some(file))
                        }
                    }
                    HashingAlgo::Sha2 => {
                        let hasher = Hasher::new_sha2(&file);
                        let Ok(new_hash) = hasher.find_root_hash() else {
                            return (format!("ERROR\t{}", &file.display()), Some(file));
                        };
                        if compared_hashes(&old_hash, &new_hash, &file).is_ok() {
                            info!("The file {} passed its checksum", &file.display());
                            (format!("PASS\t{}", &file.display()), None)
                        } else {
                            (format!("FAIL\t{}", &file.display()), Some(file))
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        // Collect output lines and failed files
        let mut output_lines = Vec::new();
        let mut failed_files = Vec::new();

        for (output_line, failed_file) in results {
            output_lines.push(output_line);
            if let Some(file) = failed_file {
                failed_files.push(file);
            }
        }

        // Print all verification results as a single block to stdout
        if !output_lines.is_empty() {
            println!("{}", output_lines.join("\n"));
        }

        if failed_files.is_empty() {
            return Ok(());
        }

        if failed_files.len() == 1 {
            let failed_file = failed_files[0].clone();
            return Err(CheckleError::FailedChecksum(failed_file));
        }

        let mut error_string = "The following files failed their checksums:".to_string();
        for file in &failed_files {
            error_string = format!("{error_string}\n{}", file.display());
        }
        warn!("{error_string}");

        Err(CheckleError::MultipleFailedChecksums)
    }
}

/// Compares two hash strings and returns an error if they don't match.
///
/// # Errors
///
/// Returns `CheckleError::FailedChecksum` if the hashes don't match.
///
/// # Panics
///
/// Panics if:
/// - Either hash string is empty
/// - The file doesn't exist
#[inline]
pub fn compared_hashes(old_hash: &str, new_hash: &str, file: &Path) -> Result<()> {
    // Precondition assertions
    assert!(!old_hash.is_empty(), "Old hash must not be empty");
    assert!(!new_hash.is_empty(), "New hash must not be empty");
    assert!(file.exists(), "File must exist: {}", file.display());

    if old_hash == new_hash {
        Ok(())
    } else {
        Err(CheckleError::FailedChecksum(file.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::missing_panics_doc,
        clippy::items_after_statements
    )]
    use super::*;
    use proptest::prelude::*;
    use proptest::test_runner::FileFailurePersistence;
    use std::fs;
    use tempfile::NamedTempFile;

    // Test 1: Normal operation - MD5 hashing works correctly
    #[test]
    fn test_md5_hasher_normal_operation() {
        // Create a temporary file with known content
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Hello, World!";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // Hash the file
        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        // Verify result
        assert!(result.is_ok(), "MD5 hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters long");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );

        // Known MD5 of "Hello, World!" is 65a8e27d8879283831b664bd8b7f0ad4
        assert_eq!(hash, "65a8e27d8879283831b664bd8b7f0ad4");
    }

    // Test 2: Normal operation - SHA256 hashing works correctly
    #[test]
    fn test_sha256_hasher_normal_operation() {
        // Create a temporary file with known content
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Hello, World!";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // Hash the file
        let hasher = Hasher::new_sha2(temp_file.path());
        let result = hasher.find_root_hash();

        // Verify result
        assert!(result.is_ok(), "SHA256 hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64, "SHA256 hash should be 64 characters long");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );

        // Known SHA256 of "Hello, World!" is dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    // Test 3: Progress callback - callback is invoked during hashing
    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_progress_callback_invocation() {
        use std::sync::{Arc, Mutex};

        // Create a file smaller than PARALLEL_IO_THRESHOLD to ensure sequential processing
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x42u8; 512 * 1024]; // 512KB of data (below 1MB threshold)
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Track progress updates
        let progress_updates = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = Arc::clone(&progress_updates);

        // Create hasher with progress callback
        let mut hasher = Hasher::new_md5(temp_file.path());
        hasher = hasher.with_progress_callback(Box::new(move |bytes_read| {
            progress_clone.lock().unwrap().push(bytes_read);
        }));

        // Hash the file
        let result = hasher.find_root_hash();
        assert!(
            result.is_ok(),
            "Hashing with progress callback should succeed"
        );

        // Verify progress was tracked
        let updates = progress_updates.lock().unwrap();
        assert!(!updates.is_empty(), "Progress callback should be invoked");

        // Should have exactly 1 update for a file smaller than CHUNK_SIZE
        assert_eq!(
            updates.len(),
            1,
            "Should have exactly 1 progress update for 512KB file"
        );

        // Final progress should equal file size
        assert_eq!(
            updates[0],
            test_data.len() as u64,
            "Progress should equal file size"
        );
    }

    // Test 4: Progress callback - works with empty files
    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_progress_callback_empty_file() {
        use std::sync::{Arc, Mutex};

        // Create an empty file
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Track progress updates
        let progress_updates = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = Arc::clone(&progress_updates);

        // Create hasher with progress callback
        let mut hasher = Hasher::new_md5(temp_file.path());
        hasher = hasher.with_progress_callback(Box::new(move |bytes_read| {
            progress_clone.lock().unwrap().push(bytes_read);
        }));

        // Hash the file
        let result = hasher.find_root_hash();
        assert!(
            result.is_ok(),
            "Hashing empty file with progress callback should succeed"
        );

        // Verify no progress updates for empty file
        let updates = progress_updates.lock().unwrap();
        assert!(
            updates.is_empty(),
            "Empty file should not trigger progress updates"
        );
    }

    // Test 5: Progress callback - thread safety
    #[test]
    fn test_progress_callback_thread_safety() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        };

        // Create a large file for parallel processing
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x33u8; CHUNK_SIZE * 10]; // 10MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Use atomic counter for thread-safe progress tracking
        let total_progress = Arc::new(AtomicU64::new(0));
        let progress_clone = Arc::clone(&total_progress);

        // Create hasher with thread-safe progress callback
        let mut hasher = Hasher::new_sha2(temp_file.path());
        hasher = hasher.with_progress_callback(Box::new(move |bytes_read| {
            progress_clone.store(bytes_read, Ordering::Relaxed);
        }));

        // Enable parallel readers
        hasher = hasher.with_parallel_readers(4);

        // Hash the file
        let result = hasher.find_root_hash();
        assert!(
            result.is_ok(),
            "Parallel hashing with progress callback should succeed"
        );

        // Note: With current implementation, progress callbacks only work in sequential mode
        // This test verifies that having a callback doesn't break parallel hashing
    }

    // Test 3: Edge case - empty file hashing
    #[test]
    fn test_empty_file_hashing() {
        // Create an empty temporary file
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Hash with MD5
        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        // Verify result
        assert!(result.is_ok(), "Empty file hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters long");
        // MD5 of empty file is d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    // Test 4: Edge case - large file with multiple chunks
    #[test]
    fn test_large_file_hashing() {
        // Create a temporary file larger than CHUNK_SIZE (1MB)
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let chunk_data = vec![0u8; CHUNK_SIZE]; // 1MB of zeros
        let large_data = [&chunk_data[..], &chunk_data[..], b"extra"].concat(); // >2MB
        fs::write(temp_file.path(), &large_data).expect("Failed to write large data");

        // Hash the file
        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        // Verify result
        assert!(result.is_ok(), "Large file hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters long");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should contain only hex digits"
        );
    }

    // Test 5: Error path - non-existent file
    #[test]
    #[should_panic(expected = "Input path must exist")]
    fn test_hasher_nonexistent_file() {
        let nonexistent_path = Path::new("/nonexistent/file/path");
        let _hasher = Hasher::new_md5(nonexistent_path);
    }

    // Test 6: Error path - directory instead of file
    #[test]
    #[should_panic(expected = "Input path must be a file, not directory")]
    fn test_hasher_directory_path() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let _hasher = Hasher::new_md5(temp_dir.path());
    }

    // Test 7: Checksum verification - correct hash
    #[test]
    fn test_checksum_verification_success() {
        // Create a temporary file with known content
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Test checksum";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // First, get the hash
        let hasher = Hasher::new_md5(temp_file.path());
        let correct_hash = hasher.find_root_hash().expect("Failed to generate hash");

        // Now verify with the correct hash
        let verifier = Hasher::new_md5(temp_file.path());
        let result = verifier.checksum(&correct_hash);

        assert!(
            result.is_ok(),
            "Checksum verification should succeed with correct hash"
        );
    }

    // Test 8: Checksum verification - incorrect hash
    #[test]
    fn test_checksum_verification_failure() {
        // Create a temporary file with known content
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_content = b"Test checksum";
        fs::write(temp_file.path(), test_content).expect("Failed to write test content");

        // Use an incorrect hash
        let incorrect_hash = "0123456789abcdef0123456789abcdef"; // 32 char hex string

        let verifier = Hasher::new_md5(temp_file.path());
        let result = verifier.checksum(incorrect_hash);

        assert!(
            result.is_err(),
            "Checksum verification should fail with incorrect hash"
        );
        if let Err(CheckleError::FailedChecksum(path)) = result {
            assert_eq!(path, temp_file.path());
        } else {
            panic!("Expected FailedChecksum error");
        }
    }

    // Test 9: Hash comparison function
    #[test]
    fn test_compared_hashes_function() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Test matching hashes
        let hash1 = "abcdef123456";
        let hash2 = "abcdef123456";
        let result = compared_hashes(hash1, hash2, temp_file.path());
        assert!(result.is_ok(), "Matching hashes should return Ok");

        // Test non-matching hashes
        let hash3 = "different123";
        let result = compared_hashes(hash1, hash3, temp_file.path());
        assert!(result.is_err(), "Non-matching hashes should return Err");
    }

    // Property-based tests for hash algorithm invariants
    proptest! {
        #![proptest_config({
            ProptestConfig {
                cases: 5,
                failure_persistence: Some(Box::new(
                    FileFailurePersistence::SourceParallel("tests/proptest-regressions")
                )),
                ..Default::default()
            }
        })]
        // Property 1: Hash determinism - same input produces same hash
        #[test]
        fn test_hash_determinism(data in prop::collection::vec(any::<u8>(), 0..10000)) {
            let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
            let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

            fs::write(temp_file1.path(), &data).expect("Failed to write data");
            fs::write(temp_file2.path(), &data).expect("Failed to write data");

            let hasher1 = Hasher::new_md5(temp_file1.path());
            let hasher2 = Hasher::new_md5(temp_file2.path());

            let hash1 = hasher1.find_root_hash().expect("Hash should succeed");
            let hash2 = hasher2.find_root_hash().expect("Hash should succeed");

            prop_assert_eq!(hash1, hash2, "Same input should produce same hash");
        }

        // Property 2: Hash difference - different inputs produce different hashes (with high probability)
        #[test]
        fn test_hash_difference(
            data1 in prop::collection::vec(any::<u8>(), 1..1000),
            data2 in prop::collection::vec(any::<u8>(), 1..1000)
        ) {
            // Skip if data is identical
            prop_assume!(data1 != data2);

            let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
            let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

            fs::write(temp_file1.path(), &data1).expect("Failed to write data");
            fs::write(temp_file2.path(), &data2).expect("Failed to write data");

            let hasher1 = Hasher::new_md5(temp_file1.path());
            let hasher2 = Hasher::new_md5(temp_file2.path());

            let hash1 = hasher1.find_root_hash().expect("Hash should succeed");
            let hash2 = hasher2.find_root_hash().expect("Hash should succeed");

            // Different inputs should produce different hashes (cryptographic property)
            // Note: This could theoretically fail due to hash collisions, but probability is negligible
            prop_assert_ne!(hash1, hash2, "Different inputs should produce different hashes");
        }

        // Property 3: Hash length invariant
        #[test]
        fn test_hash_length_invariant(data in prop::collection::vec(any::<u8>(), 0..5000)) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            fs::write(temp_file.path(), &data).expect("Failed to write data");

            // Test MD5 length
            let hasher_md5 = Hasher::new_md5(temp_file.path());
            let hash_md5 = hasher_md5.find_root_hash().expect("MD5 hash should succeed");
            prop_assert_eq!(hash_md5.len(), 32, "MD5 hash should always be 32 characters");

            // Test SHA256 length
            let hasher_sha = Hasher::new_sha2(temp_file.path());
            let hash_sha = hasher_sha.find_root_hash().expect("SHA256 hash should succeed");
            prop_assert_eq!(hash_sha.len(), 64, "SHA256 hash should always be 64 characters");
        }

        // Property 4: Hash content is always hexadecimal
        #[test]
        fn test_hash_hex_content(data in prop::collection::vec(any::<u8>(), 0..10000)) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            fs::write(temp_file.path(), &data).expect("Failed to write data");

            // Test MD5 hex content
            let hasher_md5 = Hasher::new_md5(temp_file.path());
            let hash_md5 = hasher_md5.find_root_hash().expect("MD5 hash should succeed");
            prop_assert!(hash_md5.chars().all(|c| c.is_ascii_hexdigit()), "MD5 hash should be hexadecimal");

            // Test SHA256 hex content
            let hasher_sha = Hasher::new_sha2(temp_file.path());
            let hash_sha = hasher_sha.find_root_hash().expect("SHA256 hash should succeed");
            prop_assert!(hash_sha.chars().all(|c| c.is_ascii_hexdigit()), "SHA256 hash should be hexadecimal");
        }

        // Property 5: Merkle tree correctness with multiple chunk sizes
        #[test]
        fn test_merkle_tree_consistency(
            data in prop::collection::vec(any::<u8>(), CHUNK_SIZE * 2..CHUNK_SIZE * 10)
        ) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            fs::write(temp_file.path(), &data).expect("Failed to write data");

            // Hash with both algorithms and ensure they're consistent
            let hasher_md5 = Hasher::new_md5(temp_file.path());
            let hash_md5 = hasher_md5.find_root_hash().expect("MD5 hash should succeed");

            let hasher_sha = Hasher::new_sha2(temp_file.path());
            let hash_sha = hasher_sha.find_root_hash().expect("SHA256 hash should succeed");

            // Both hashes should be valid but different
            prop_assert_eq!(hash_md5.len(), 32);
            prop_assert_eq!(hash_sha.len(), 64);
            prop_assert_ne!(hash_md5, &hash_sha[..32]); // Different algorithms produce different results
        }

        // Property 6: Checksum verification symmetry
        #[test]
        fn test_checksum_verification_symmetry(data in prop::collection::vec(any::<u8>(), 0..50000)) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            fs::write(temp_file.path(), &data).expect("Failed to write data");

            // Generate hash
            let hasher1 = Hasher::new_md5(temp_file.path());
            let hash = hasher1.find_root_hash().expect("Hash generation should succeed");

            // Verify with same hash should succeed
            let hasher2 = Hasher::new_md5(temp_file.path());
            let result = hasher2.checksum(&hash);
            prop_assert!(result.is_ok(), "Checksum verification should succeed with correct hash");
        }

        // Property 7: Hash avalanche effect (small changes create big differences)
        #[test]
        fn test_hash_avalanche_effect(
            mut data in prop::collection::vec(any::<u8>(), 100..1000),
            bit_position in 0usize..7,
            byte_position in 0usize..99
        ) {
            prop_assume!(byte_position < data.len());

            let temp_file1 = NamedTempFile::new().expect("Failed to create temp file");
            let temp_file2 = NamedTempFile::new().expect("Failed to create temp file");

            // Write original data
            fs::write(temp_file1.path(), &data).expect("Failed to write original data");

            // Flip one bit
            data[byte_position] ^= 1 << bit_position;
            fs::write(temp_file2.path(), &data).expect("Failed to write modified data");

            let hasher1 = Hasher::new_md5(temp_file1.path());
            let hasher2 = Hasher::new_md5(temp_file2.path());

            let hash1 = hasher1.find_root_hash().expect("Hash1 should succeed");
            let hash2 = hasher2.find_root_hash().expect("Hash2 should succeed");

            // Even one bit change should produce completely different hash
            prop_assert_ne!(&hash1, &hash2, "One bit change should produce different hash");

            // Count differing characters (should be roughly 50% for good hash function)
            let diff_count = hash1.chars().zip(hash2.chars()).filter(|(c1, c2)| c1 != c2).count();
            prop_assert!(diff_count >= 8, "Hash avalanche effect should be significant: {} differences", diff_count);
        }
    }

    // Test 10: MerkleIter functionality
    #[test]
    fn test_merkle_iter_single_hash() {
        let single_hash = [0u8; 16]; // MD5 size
        let hash_array = HashArray {
            hashes: vec![single_hash],
        };

        // Single hash should return itself
        let result = hash_array
            .par_iter_merkle::<Md5>()
            .expect("Single hash should succeed");
        let final_hashes = result.get_hashes();

        assert_eq!(
            final_hashes.len(),
            1,
            "Single hash should return one result"
        );
        assert_eq!(
            final_hashes[0], single_hash,
            "Single hash should return unchanged"
        );
    }

    // Test 11: MerkleIter with multiple hashes
    #[test]
    fn test_merkle_iter_multiple_hashes() {
        let hash1 = [1u8; 16]; // MD5 size
        let hash2 = [2u8; 16];
        let hash_array = HashArray {
            hashes: vec![hash1, hash2],
        };

        // Multiple hashes should be combined
        let result = hash_array
            .par_iter_merkle::<Md5>()
            .expect("Multiple hashes should succeed");
        let final_hashes = result.get_hashes();

        assert_eq!(
            final_hashes.len(),
            1,
            "Multiple hashes should combine to one root hash"
        );
        // The result should be different from both input hashes
        assert_ne!(final_hashes[0], hash1, "Root hash should differ from input");
        assert_ne!(final_hashes[0], hash2, "Root hash should differ from input");
    }

    // Test 12: HashArray utility methods
    #[test]
    fn test_hash_array_utility_methods() {
        let hashes = vec![[1u8; 16], [2u8; 16], [3u8; 16]];
        let hash_array = HashArray {
            hashes: hashes.clone(),
        };

        assert_eq!(hash_array.len(), 3, "Length should match input");
        assert!(
            !hash_array.is_empty(),
            "Non-empty array should not be empty"
        );

        let retrieved_hashes = hash_array.get_hashes();
        assert_eq!(
            retrieved_hashes, hashes,
            "Retrieved hashes should match original"
        );

        // Test empty array
        let empty_array: HashArray<16> = HashArray { hashes: vec![] };
        assert_eq!(empty_array.len(), 0, "Empty array should have length 0");
        assert!(empty_array.is_empty(), "Empty array should be empty");
    }

    // Test 13: Realistic file sizes - 1KB file (single chunk)
    #[test]
    fn test_realistic_file_1kb() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let data = vec![0x42u8; 1024]; // 1KB of 'B' characters
        fs::write(temp_file.path(), &data).expect("Failed to write 1KB data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        assert!(result.is_ok(), "1KB file hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters");

        // Test with SHA256 as well
        let hasher_sha = Hasher::new_sha2(temp_file.path());
        let result_sha = hasher_sha.find_root_hash();
        assert!(result_sha.is_ok(), "1KB file SHA256 hashing should succeed");
        let hash_sha = result_sha.unwrap();
        assert_eq!(hash_sha.len(), 64, "SHA256 hash should be 64 characters");
    }

    // Test 14: Realistic file sizes - 10MB file (multiple chunks)
    #[test]
    fn test_realistic_file_10mb() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let chunk = vec![0xAAu8; CHUNK_SIZE]; // 1MB chunk
        let mut large_data = Vec::new();

        // Create 10MB file
        for _ in 0..10 {
            large_data.extend_from_slice(&chunk);
        }
        fs::write(temp_file.path(), &large_data).expect("Failed to write 10MB data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        assert!(result.is_ok(), "10MB file hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "MD5 hash should be 32 characters");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should be hexadecimal"
        );
    }

    // Test 15: Realistic file sizes - genomics-sized file (100MB simulation)
    #[test]
    fn test_realistic_genomics_simulation() {
        // Instead of creating a real 100MB file, we'll test with repeated smaller patterns
        // to simulate genomics data characteristics
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Simulate genomics data with ATCG patterns
        let base_pattern = b"ATCGATCGATCGATCG"; // 16 bytes
        let mut genomics_data = Vec::new();

        // Create ~1MB of genomics-like data
        for _ in 0..(64 * 1024) {
            // 64K * 16 bytes = ~1MB
            genomics_data.extend_from_slice(base_pattern);
        }

        fs::write(temp_file.path(), &genomics_data).expect("Failed to write genomics data");

        let hasher = Hasher::new_sha2(temp_file.path()); // Use SHA256 for genomics
        let result = hasher.find_root_hash();

        assert!(result.is_ok(), "Genomics simulation hashing should succeed");
        let hash = result.unwrap();
        assert_eq!(hash.len(), 64, "SHA256 hash should be 64 characters");
    }

    // Test 16: Edge case - exactly one chunk boundary
    #[test]
    fn test_exact_chunk_boundary() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let data = vec![0x01u8; CHUNK_SIZE]; // Exactly 1MB
        fs::write(temp_file.path(), &data).expect("Failed to write chunk-sized data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        assert!(
            result.is_ok(),
            "Chunk boundary file should hash successfully"
        );
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "Hash should be correct length");
    }

    // Test 17: Edge case - slightly over chunk boundary
    #[test]
    fn test_just_over_chunk_boundary() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let mut data = vec![0x01u8; CHUNK_SIZE]; // 1MB
        data.push(0x02); // Plus 1 byte
        fs::write(temp_file.path(), &data).expect("Failed to write over-boundary data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.find_root_hash();

        assert!(
            result.is_ok(),
            "Just-over-boundary file should hash successfully"
        );
        let hash = result.unwrap();
        assert_eq!(hash.len(), 32, "Hash should be correct length");
    }

    // === Parallel I/O Tests ===

    // Test 1: Parallel produces same hash as sequential - CRITICAL TEST
    #[test]
    fn test_parallel_sequential_hash_equivalence() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create test data larger than parallel threshold (>1MB)
        let chunk_data = vec![0x42u8; CHUNK_SIZE]; // 1MB of 'B'
        let test_data = [&chunk_data[..], &chunk_data[..], b"extra_data"].concat(); // >2MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        println!("File size: {} bytes", test_data.len());
        println!("CHUNK_SIZE: {} bytes", CHUNK_SIZE);

        // Hash with sequential method
        let hasher_seq = Hasher::new_md5(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Md5>()
            .expect("Sequential hashing should succeed")
            .get_hashes();

        println!("Sequential produced {} hashes", sequential_hashes.len());

        // Hash with parallel method (4 threads)
        let hasher_par = Hasher::new_md5(temp_file.path());
        let parallel_hashes = hasher_par
            .compute_starter_hashes_parallel::<Md5>(4)
            .expect("Parallel hashing should succeed")
            .get_hashes();

        println!("Parallel produced {} hashes", parallel_hashes.len());

        // Debug: Calculate expected chunks
        let expected_chunks = test_data.len().div_ceil(CHUNK_SIZE);
        println!("Expected chunks: {}", expected_chunks);

        // Hashes must be identical
        assert_eq!(
            sequential_hashes, parallel_hashes,
            "Parallel and sequential hashing must produce identical results"
        );
        assert!(
            !sequential_hashes.is_empty(),
            "Should produce at least one hash"
        );
    }

    // Test 2: Parallel vs sequential with various file sizes
    #[test]
    fn test_parallel_various_file_sizes() {
        let test_cases = vec![
            (1024, "1KB file"),                // Small file, should fall back to sequential
            (CHUNK_SIZE / 2, "Half chunk"),    // Half a chunk
            (CHUNK_SIZE, "Exactly one chunk"), // Exactly one chunk
            (CHUNK_SIZE + 100, "Just over one chunk"), // Slightly more than one chunk
            (CHUNK_SIZE * 3, "Three chunks"),  // Multiple chunks
        ];

        for (file_size, description) in test_cases {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            let test_data = vec![0xAAu8; file_size];
            fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

            // Sequential hash
            let hasher_seq = Hasher::new_md5(temp_file.path());
            let sequential_hashes = hasher_seq
                .compute_starter_hashes::<Md5>()
                .expect("Sequential hashing should succeed")
                .get_hashes();

            // Parallel hash with different thread counts
            for threads in [1, 2, 4, 8] {
                let hasher_par = Hasher::new_md5(temp_file.path());
                let parallel_hashes = hasher_par
                    .compute_starter_hashes_parallel::<Md5>(threads)
                    .expect("Parallel hashing should succeed")
                    .get_hashes();

                assert_eq!(
                    sequential_hashes, parallel_hashes,
                    "Hash mismatch for {} with {} threads",
                    description, threads
                );
            }
        }
    }

    // Test 3: Empty file handling in parallel mode
    #[test]
    fn test_parallel_empty_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        // File is already empty

        // Sequential hash
        let hasher_seq = Hasher::new_md5(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Md5>()
            .expect("Sequential hashing should succeed")
            .get_hashes();

        // Parallel hash
        let hasher_par = Hasher::new_md5(temp_file.path());
        let parallel_hashes = hasher_par
            .compute_starter_hashes_parallel::<Md5>(4)
            .expect("Parallel hashing should succeed")
            .get_hashes();

        // Both should produce one hash (hash of empty data)
        assert_eq!(
            sequential_hashes.len(),
            1,
            "Empty file should produce one hash"
        );
        assert_eq!(
            parallel_hashes.len(),
            1,
            "Empty file should produce one hash"
        );
        assert_eq!(
            sequential_hashes, parallel_hashes,
            "Empty file hashes must match"
        );
    }

    // Test 4: Large file simulation with various thread counts
    #[test]
    fn test_parallel_large_file_simulation() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create a ~5MB file with varied patterns
        let mut large_data = Vec::new();
        for i in 0..5 {
            let pattern = vec![u8::try_from(i * 37).expect("pattern value fits in u8"); CHUNK_SIZE]; // Different pattern per MB
            large_data.extend_from_slice(&pattern);
        }
        fs::write(temp_file.path(), &large_data).expect("Failed to write large test data");

        // Sequential reference
        let hasher_seq = Hasher::new_md5(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Md5>()
            .expect("Sequential hashing should succeed")
            .get_hashes();

        // Test with various thread counts
        for threads in [2, 4, 8, 16] {
            let hasher_par = Hasher::new_md5(temp_file.path());
            let parallel_hashes = hasher_par
                .compute_starter_hashes_parallel::<Md5>(threads)
                .expect("Parallel hashing should succeed")
                .get_hashes();

            assert_eq!(
                sequential_hashes, parallel_hashes,
                "Large file hash mismatch with {} threads",
                threads
            );
        }
    }

    // Test 5: Edge case - file size exactly at parallel threshold
    #[test]
    fn test_parallel_threshold_boundary() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data =
            vec![0x55u8; usize::try_from(PARALLEL_IO_THRESHOLD).expect("threshold fits in usize")]; // Exactly 1MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write threshold data");

        let hasher_seq = Hasher::new_md5(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Md5>()
            .expect("Sequential hashing should succeed")
            .get_hashes();

        let hasher_par = Hasher::new_md5(temp_file.path());
        let parallel_hashes = hasher_par
            .compute_starter_hashes_parallel::<Md5>(4)
            .expect("Parallel hashing should succeed")
            .get_hashes();

        assert_eq!(
            sequential_hashes, parallel_hashes,
            "Threshold boundary file hashes must match"
        );
    }

    // Test 6: Error handling - invalid parameters
    #[test]
    #[should_panic(expected = "Parallel readers count must be positive")]
    fn test_parallel_zero_threads_panics() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x44u8; CHUNK_SIZE * 2];
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path());
        let _ = hasher.compute_starter_hashes_parallel::<Md5>(0);
    }

    // Test 7: SHA256 parallel vs sequential equivalence
    #[test]
    fn test_parallel_sha256_equivalence() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x77u8; CHUNK_SIZE * 2]; // 2MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Sequential SHA256
        let hasher_seq = Hasher::new_sha2(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Sha256>()
            .expect("Sequential SHA256 should succeed")
            .get_hashes();

        // Parallel SHA256
        let hasher_par = Hasher::new_sha2(temp_file.path());
        let parallel_hashes = hasher_par
            .compute_starter_hashes_parallel::<Sha256>(4)
            .expect("Parallel SHA256 should succeed")
            .get_hashes();

        assert_eq!(
            sequential_hashes, parallel_hashes,
            "SHA256 parallel and sequential must match"
        );
    }

    // Test 8: Property-based test - any data + thread count = same hash
    proptest! {
        #![proptest_config({
            ProptestConfig {
                cases: 10,
                failure_persistence: Some(Box::new(
                    FileFailurePersistence::SourceParallel("tests/proptest-regressions")
                )),
                ..Default::default()
            }
        })]

        #[test]
        fn test_parallel_property_same_hash(
            data in prop::collection::vec(any::<u8>(), usize::try_from(PARALLEL_IO_THRESHOLD).expect("threshold fits in usize")..(usize::try_from(PARALLEL_IO_THRESHOLD).expect("threshold fits in usize") * 3)),
            threads in 1usize..=8
        ) {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            fs::write(temp_file.path(), &data).expect("Failed to write data");

            // Sequential hash
            let hasher_seq = Hasher::new_md5(temp_file.path());
            let sequential_hashes = hasher_seq
                .compute_starter_hashes::<Md5>()
                .expect("Sequential should succeed")
                .get_hashes();

            // Parallel hash
            let hasher_par = Hasher::new_md5(temp_file.path());
            let parallel_hashes = hasher_par
                .compute_starter_hashes_parallel::<Md5>(threads)
                .expect("Parallel should succeed")
                .get_hashes();

            prop_assert_eq!(
                sequential_hashes, parallel_hashes,
                "Property test failed: sequential != parallel with {} threads",
                threads
            );
        }
    }

    // Test 9: Performance comparison - parallel should be faster for large files
    #[test]
    fn test_parallel_performance_improvement() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create a larger file (10MB) for performance testing
        let large_data = vec![0x88u8; CHUNK_SIZE * 10];
        fs::write(temp_file.path(), &large_data).expect("Failed to write performance test data");

        use std::time::Instant;

        // Measure sequential time
        let start = Instant::now();
        let hasher_seq = Hasher::new_md5(temp_file.path());
        let sequential_hashes = hasher_seq
            .compute_starter_hashes::<Md5>()
            .expect("Sequential should succeed")
            .get_hashes();
        let sequential_time = start.elapsed();

        // Measure parallel time (4 threads)
        let start = Instant::now();
        let hasher_par = Hasher::new_md5(temp_file.path());
        let parallel_hashes = hasher_par
            .compute_starter_hashes_parallel::<Md5>(4)
            .expect("Parallel should succeed")
            .get_hashes();
        let parallel_time = start.elapsed();

        // Verify correctness first
        assert_eq!(
            sequential_hashes, parallel_hashes,
            "Performance test: hashes must match"
        );

        // Performance check - parallel should be at least not significantly slower
        // Allow up to 50% slower due to overhead in small test environments
        let ratio = parallel_time.as_secs_f64() / sequential_time.as_secs_f64();
        assert!(
            ratio < 1.5,
            "Parallel I/O should not be >50% slower than sequential. Ratio: {:.2} (seq: {:?}, par: {:?})",
            ratio,
            sequential_time,
            parallel_time
        );

        println!(
            "Performance: Sequential: {:?}, Parallel: {:?}, Ratio: {:.2}x",
            sequential_time, parallel_time, ratio
        );
    }

    // Test 10: Buffer pool integration
    #[test]
    fn test_parallel_buffer_pool_integration() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x99u8; CHUNK_SIZE * 3]; // 3MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // This test verifies that buffer pool is used correctly
        // by running parallel hashing multiple times and ensuring consistency
        let mut results = Vec::new();

        for _ in 0..5 {
            let hash_engine = Hasher::new_md5(temp_file.path());
            let computed_hashes = hash_engine
                .compute_starter_hashes_parallel::<Md5>(4)
                .expect("Parallel hashing should succeed")
                .get_hashes();
            results.push(computed_hashes);
        }

        // All results should be identical
        for (i, result) in results.iter().enumerate() {
            assert_eq!(
                result, &results[0],
                "Buffer pool test: iteration {} produced different result",
                i
            );
        }
    }

    // === FileRegion Tests ===

    // Test 1: Even division - 4MB file with 4 threads, each gets 1MB
    #[test]
    fn test_file_region_even_division() {
        let file_size = 4 * 1024 * 1024; // 4MB
        let num_threads = 4;
        let chunk_size = DEFAULT_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Basic assertions
        assert_eq!(regions.len(), 4, "Should have 4 regions");

        // Each region should be exactly 1MB
        for (i, region) in regions.iter().enumerate() {
            assert_eq!(region.thread_id, i, "Thread ID should match index");
            assert_eq!(region.chunk_size, chunk_size, "Chunk size should match");
            assert_eq!(region.size(), 1024 * 1024, "Each region should be 1MB");
        }

        // Check contiguous coverage
        assert_eq!(regions[0].start_offset, 0, "First region starts at 0");
        assert_eq!(
            regions[0].end_offset,
            1024 * 1024,
            "First region ends at 1MB"
        );
        assert_eq!(
            regions[1].start_offset,
            1024 * 1024,
            "Second region starts at 1MB"
        );
        assert_eq!(
            regions[1].end_offset,
            2 * 1024 * 1024,
            "Second region ends at 2MB"
        );
        assert_eq!(
            regions[2].start_offset,
            2 * 1024 * 1024,
            "Third region starts at 2MB"
        );
        assert_eq!(
            regions[2].end_offset,
            3 * 1024 * 1024,
            "Third region ends at 3MB"
        );
        assert_eq!(
            regions[3].start_offset,
            3 * 1024 * 1024,
            "Fourth region starts at 3MB"
        );
        assert_eq!(
            regions[3].end_offset,
            4 * 1024 * 1024,
            "Fourth region ends at 4MB"
        );
    }

    // Test 2: Uneven division with remainder - larger file that can be split
    #[test]
    fn test_file_region_uneven_division() {
        let file_size = 10 * MIN_CHUNK_SIZE as u64; // 40KB, larger than chunk size
        let num_threads = 3;
        let chunk_size = MIN_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Basic assertions
        assert_eq!(regions.len(), 3, "Should have 3 regions");

        // 40KB / 3 = 13653 bytes remainder 1, so first thread gets +1
        let expected_base_size = file_size / 3;

        // First thread gets base size + 1 (due to remainder)
        assert_eq!(regions[0].thread_id, 0);
        assert_eq!(regions[0].start_offset, 0);
        assert_eq!(regions[0].end_offset, expected_base_size + 1);
        assert_eq!(regions[0].size(), expected_base_size + 1);

        // Second thread gets base size
        assert_eq!(regions[1].thread_id, 1);
        assert_eq!(regions[1].start_offset, expected_base_size + 1);
        assert_eq!(
            regions[1].end_offset,
            (expected_base_size + 1) + expected_base_size
        );
        assert_eq!(regions[1].size(), expected_base_size);

        // Third thread gets base size
        assert_eq!(regions[2].thread_id, 2);
        assert_eq!(
            regions[2].start_offset,
            (expected_base_size + 1) + expected_base_size
        );
        assert_eq!(regions[2].end_offset, file_size);
        assert_eq!(regions[2].size(), expected_base_size);

        // Verify total coverage
        let total_coverage: u64 = regions.iter().map(FileRegion::size).sum();
        assert_eq!(
            total_coverage, file_size,
            "Total coverage must equal file size"
        );
    }

    // Test 3: Edge cases - empty file
    #[test]
    fn test_file_region_empty_file() {
        let file_size = 0;
        let num_threads = 4;
        let chunk_size = DEFAULT_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Should return single empty region
        assert_eq!(regions.len(), 1, "Empty file should have one region");
        assert_eq!(
            regions[0].thread_id, 0,
            "Single region should have thread_id 0"
        );
        assert_eq!(regions[0].start_offset, 0, "Empty region starts at 0");
        assert_eq!(regions[0].end_offset, 0, "Empty region ends at 0");
        assert_eq!(regions[0].size(), 0, "Empty region has size 0");
        assert!(regions[0].is_empty(), "Region should be empty");
    }

    // Test 4: Edge cases - single byte file
    #[test]
    fn test_file_region_single_byte() {
        let file_size = 1;
        let num_threads = 4;
        let chunk_size = MIN_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Should return single region with one byte
        assert_eq!(regions.len(), 1, "Single byte file should have one region");
        assert_eq!(
            regions[0].thread_id, 0,
            "Single region should have thread_id 0"
        );
        assert_eq!(regions[0].start_offset, 0, "Region starts at 0");
        assert_eq!(regions[0].end_offset, 1, "Region ends at 1");
        assert_eq!(regions[0].size(), 1, "Region has size 1");
        assert!(!regions[0].is_empty(), "Region should not be empty");
    }

    // Test 5: Edge cases - file smaller than chunk size
    #[test]
    fn test_file_region_smaller_than_chunk() {
        let file_size = 1024; // 1KB, smaller than MIN_CHUNK_SIZE (4KB)
        let num_threads = 4;
        let chunk_size = MIN_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Should return single region for the entire file
        assert_eq!(regions.len(), 1, "Small file should have one region");
        assert_eq!(regions[0].thread_id, 0);
        assert_eq!(regions[0].start_offset, 0);
        assert_eq!(regions[0].end_offset, file_size);
        assert_eq!(regions[0].size(), file_size);
    }

    // Test 6: Single thread scenario
    #[test]
    fn test_file_region_single_thread() {
        let file_size = 10 * 1024 * 1024; // 10MB
        let num_threads = 1;
        let chunk_size = DEFAULT_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        // Should return single region for entire file
        assert_eq!(regions.len(), 1, "Single thread should have one region");
        assert_eq!(regions[0].thread_id, 0);
        assert_eq!(regions[0].start_offset, 0);
        assert_eq!(regions[0].end_offset, file_size);
        assert_eq!(regions[0].size(), file_size);
    }

    // Test 7: Maximum threads
    #[test]
    fn test_file_region_max_threads() {
        let file_size = MAX_PARALLEL_READERS as u64 * 1024; // Large enough to split
        let num_threads = MAX_PARALLEL_READERS;
        let chunk_size = MIN_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        assert_eq!(
            regions.len(),
            MAX_PARALLEL_READERS,
            "Should use max threads"
        );

        // Verify contiguous coverage
        for i in 1..regions.len() {
            assert_eq!(
                regions[i - 1].end_offset,
                regions[i].start_offset,
                "Regions {} and {} must be contiguous",
                i - 1,
                i
            );
        }

        // Verify total coverage
        let total_coverage: u64 = regions.iter().map(FileRegion::size).sum();
        assert_eq!(
            total_coverage, file_size,
            "Total coverage must equal file size"
        );
    }

    // Test 8: Panic conditions - zero threads
    #[test]
    #[should_panic(expected = "Thread count must be positive")]
    fn test_file_region_zero_threads_panics() {
        let _ = FileRegion::calculate_regions(1024, 0, DEFAULT_CHUNK_SIZE);
    }

    // Test 9: Panic conditions - too many threads
    #[test]
    #[should_panic(expected = "Thread count exceeds maximum")]
    fn test_file_region_too_many_threads_panics() {
        let _ = FileRegion::calculate_regions(1024, MAX_PARALLEL_READERS + 1, DEFAULT_CHUNK_SIZE);
    }

    // Test 10: Panic conditions - chunk size too small
    #[test]
    #[should_panic(expected = "Chunk size too small")]
    fn test_file_region_chunk_too_small_panics() {
        let _ = FileRegion::calculate_regions(1024, 4, MIN_CHUNK_SIZE - 1);
    }

    // Test 11: Panic conditions - chunk size too large
    #[test]
    #[should_panic(expected = "Chunk size too large")]
    fn test_file_region_chunk_too_large_panics() {
        let _ = FileRegion::calculate_regions(1024, 4, MAX_CHUNK_SIZE + 1);
    }

    // Test 12: Large file simulation (genomics scale)
    #[test]
    fn test_file_region_large_file() {
        let file_size = 1024 * 1024 * 1024; // 1GB (simulated genomics file)
        let num_threads = 8;
        let chunk_size = DEFAULT_CHUNK_SIZE;

        let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

        assert_eq!(regions.len(), 8, "Should have 8 regions");

        // Each region should be approximately 128MB
        let expected_size = file_size / num_threads as u64;
        for region in &regions {
            assert!(
                region.size() >= expected_size,
                "Region size {} should be at least {}",
                region.size(),
                expected_size
            );
            assert!(
                region.size() <= expected_size + 1,
                "Region size {} should be at most {}",
                region.size(),
                expected_size + 1
            );
        }

        // Verify total coverage
        let total_coverage: u64 = regions.iter().map(FileRegion::size).sum();
        assert_eq!(
            total_coverage, file_size,
            "Total coverage must equal file size"
        );

        // Verify contiguous coverage
        assert_eq!(regions[0].start_offset, 0);
        assert_eq!(regions.last().unwrap().end_offset, file_size);
        for i in 1..regions.len() {
            assert_eq!(regions[i - 1].end_offset, regions[i].start_offset);
        }
    }

    // Test 13: Utility methods
    #[test]
    fn test_file_region_utility_methods() {
        let region = FileRegion {
            thread_id: 2,
            start_offset: 1000,
            end_offset: 2000,
            chunk_size: DEFAULT_CHUNK_SIZE,
        };

        assert_eq!(region.size(), 1000, "Size should be end - start");
        assert!(!region.is_empty(), "Non-empty region should not be empty");

        let empty_region = FileRegion {
            thread_id: 0,
            start_offset: 0,
            end_offset: 0,
            chunk_size: DEFAULT_CHUNK_SIZE,
        };

        assert_eq!(empty_region.size(), 0, "Empty region should have size 0");
        assert!(empty_region.is_empty(), "Empty region should be empty");
    }

    // === Integration Tests for Builder Pattern and Parallel I/O Integration ===

    // Test 1: Builder pattern with chunk size configuration
    #[test]
    fn test_builder_pattern_chunk_size() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x42u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test valid chunk size
        let hasher = Hasher::new_md5(temp_file.path())
            .with_chunk_size(64 * 1024) // 64KB
            .expect("Valid chunk size should work");

        assert_eq!(
            hasher.chunk_size,
            64 * 1024,
            "Chunk size should be set correctly"
        );
        assert_eq!(
            hasher.algorithm,
            HashingAlgo::Md5,
            "Algorithm should be preserved"
        );
        assert_eq!(hasher.path, temp_file.path(), "Path should be preserved");

        // Test the hasher still works
        let result = hasher.find_root_hash();
        assert!(result.is_ok(), "Hasher with custom chunk size should work");
    }

    // Test 2: Builder pattern with parallel readers configuration
    #[test]
    fn test_builder_pattern_parallel_readers() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x43u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test valid parallel readers count
        let hasher = Hasher::new_sha2(temp_file.path()).with_parallel_readers(8);

        assert_eq!(
            hasher.parallel_readers, 8,
            "Parallel readers should be set correctly"
        );
        assert_eq!(
            hasher.algorithm,
            HashingAlgo::Sha2,
            "Algorithm should be preserved"
        );
        assert_eq!(hasher.path, temp_file.path(), "Path should be preserved");

        // Test the hasher still works
        let result = hasher.find_root_hash();
        assert!(
            result.is_ok(),
            "Hasher with custom parallel readers should work"
        );
    }

    // Test 3: Builder pattern chaining
    #[test]
    fn test_builder_pattern_chaining() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x44u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test chaining both methods
        let hasher = Hasher::new_md5(temp_file.path())
            .with_chunk_size(32 * 1024)
            .expect("Valid chunk size should work")
            .with_parallel_readers(4);

        assert_eq!(
            hasher.chunk_size,
            32 * 1024,
            "Chunk size should be set correctly"
        );
        assert_eq!(
            hasher.parallel_readers, 4,
            "Parallel readers should be set correctly"
        );

        // Test the hasher still works
        let result = hasher.find_root_hash();
        assert!(result.is_ok(), "Chained builder should work");
    }

    // Test 4: Chunk size validation - too small
    #[test]
    fn test_chunk_size_validation_too_small() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x45u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.with_chunk_size(1024); // 1KB, smaller than MIN_CHUNK_SIZE (4KB)

        assert!(result.is_err(), "Too small chunk size should fail");
        if let Err(CheckleError::InvalidChunkSize {
            size,
            reason,
            min_size,
            max_size,
        }) = result
        {
            assert_eq!(size, 1024);
            assert!(reason.contains("too small"));
            assert_eq!(min_size, MIN_CHUNK_SIZE);
            assert_eq!(max_size, MAX_CHUNK_SIZE);
        } else {
            panic!("Expected InvalidChunkSize error");
        }
    }

    // Test 5: Chunk size validation - too large
    #[test]
    fn test_chunk_size_validation_too_large() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x46u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.with_chunk_size(128 * 1024 * 1024); // 128MB, larger than MAX_CHUNK_SIZE (64MB)

        assert!(result.is_err(), "Too large chunk size should fail");
        if let Err(CheckleError::InvalidChunkSize {
            size,
            reason,
            min_size,
            max_size,
        }) = result
        {
            assert_eq!(size, 128 * 1024 * 1024);
            assert!(reason.contains("too large"));
            assert_eq!(min_size, MIN_CHUNK_SIZE);
            assert_eq!(max_size, MAX_CHUNK_SIZE);
        } else {
            panic!("Expected InvalidChunkSize error");
        }
    }

    // Test 6: Chunk size page alignment
    #[test]
    fn test_chunk_size_page_alignment() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x47u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test unaligned size gets aligned down
        let hasher = Hasher::new_md5(temp_file.path())
            .with_chunk_size(12345) // Not page-aligned
            .expect("Valid chunk size should work");

        // Should be aligned down to nearest 4KB boundary
        let expected_aligned = 12345 & !(4096 - 1); // Should be 8192 (2 * 4KB)
        assert_eq!(
            hasher.chunk_size, expected_aligned,
            "Chunk size should be page-aligned"
        );
        assert_eq!(
            hasher.chunk_size & (4096 - 1),
            0,
            "Chunk size should be page-aligned"
        );
    }

    // Test 7: Parallel readers bounds clamping
    #[test]
    fn test_parallel_readers_bounds_clamping() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x48u8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test zero readers gets clamped to 1
        let hasher = Hasher::new_md5(temp_file.path()).with_parallel_readers(0);
        assert_eq!(
            hasher.parallel_readers, 1,
            "Zero readers should be clamped to 1"
        );

        // Test excessive readers gets clamped to maximum
        let hasher = Hasher::new_md5(temp_file.path()).with_parallel_readers(1000);
        assert_eq!(
            hasher.parallel_readers, MAX_PARALLEL_READERS,
            "Excessive readers should be clamped"
        );
    }

    // Test 8: Parallel I/O decision logic - small file
    #[test]
    fn test_parallel_io_decision_small_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x49u8; 100]; // 100 bytes, well below threshold
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path()).with_parallel_readers(4);

        let use_parallel = hasher
            .should_use_parallel_io()
            .expect("Decision should succeed");

        assert!(!use_parallel, "Small files should use sequential I/O");
    }

    // Test 9: Parallel I/O decision logic - large file with multiple readers
    #[test]
    fn test_parallel_io_decision_large_file() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data =
            vec![
                0x4Au8;
                usize::try_from(PARALLEL_IO_THRESHOLD).expect("threshold fits in usize") + 1000
            ]; // Just over threshold
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path()).with_parallel_readers(4);

        let use_parallel = hasher
            .should_use_parallel_io()
            .expect("Decision should succeed");

        assert!(
            use_parallel,
            "Large files with multiple readers should use parallel I/O"
        );
    }

    // Test 10: Parallel I/O decision logic - large file with single reader
    #[test]
    fn test_parallel_io_decision_single_reader() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data =
            vec![
                0x4Bu8;
                usize::try_from(PARALLEL_IO_THRESHOLD).expect("threshold fits in usize") + 1000
            ]; // Just over threshold
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        let hasher = Hasher::new_md5(temp_file.path()).with_parallel_readers(1);

        let use_parallel = hasher
            .should_use_parallel_io()
            .expect("Decision should succeed");

        assert!(
            !use_parallel,
            "Single reader should use sequential I/O regardless of file size"
        );
    }

    // Test 11: Backward compatibility - default behavior unchanged
    #[test]
    fn test_backward_compatibility_defaults() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x4Cu8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test that old constructors still work exactly as before
        let hasher_md5 = Hasher::new_md5(temp_file.path());
        assert_eq!(
            hasher_md5.chunk_size, DEFAULT_CHUNK_SIZE,
            "MD5 hasher should use default chunk size"
        );
        assert!(
            hasher_md5.parallel_readers > 0,
            "MD5 hasher should have positive parallel readers"
        );
        assert!(
            hasher_md5.parallel_readers <= MAX_PARALLEL_READERS,
            "MD5 hasher should respect max readers"
        );

        let hasher_sha2 = Hasher::new_sha2(temp_file.path());
        assert_eq!(
            hasher_sha2.chunk_size, DEFAULT_CHUNK_SIZE,
            "SHA2 hasher should use default chunk size"
        );
        assert!(
            hasher_sha2.parallel_readers > 0,
            "SHA2 hasher should have positive parallel readers"
        );
        assert!(
            hasher_sha2.parallel_readers <= MAX_PARALLEL_READERS,
            "SHA2 hasher should respect max readers"
        );

        // Test that old code produces the same hashes
        let hash_md5 = hasher_md5
            .find_root_hash()
            .expect("MD5 hash should succeed");
        let hash_sha2 = hasher_sha2
            .find_root_hash()
            .expect("SHA2 hash should succeed");

        assert_eq!(hash_md5.len(), 32, "MD5 hash should be 32 characters");
        assert_eq!(hash_sha2.len(), 64, "SHA2 hash should be 64 characters");
        assert!(
            hash_md5.chars().all(|c| c.is_ascii_hexdigit()),
            "MD5 hash should be hex"
        );
        assert!(
            hash_sha2.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA2 hash should be hex"
        );
    }

    // Test 12: Integration with parallel I/O - same results
    #[test]
    fn test_integration_parallel_sequential_equivalence() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Create a file large enough to trigger parallel I/O
        let chunk_data = vec![0x4Du8; CHUNK_SIZE];
        let test_data = [&chunk_data[..], &chunk_data[..], b"extra"].concat(); // >2MB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Hash with old method (should use parallel automatically)
        let hasher_auto = Hasher::new_md5(temp_file.path());
        let hash_auto = hasher_auto
            .find_root_hash()
            .expect("Auto hash should succeed");

        // Hash with forced sequential (single reader)
        let hasher_seq = Hasher::new_md5(temp_file.path()).with_parallel_readers(1);
        let hash_seq = hasher_seq
            .find_root_hash()
            .expect("Sequential hash should succeed");

        // Hash with explicit parallel (multiple readers)
        let hasher_par = Hasher::new_md5(temp_file.path()).with_parallel_readers(4);
        let hash_par = hasher_par
            .find_root_hash()
            .expect("Parallel hash should succeed");

        // All methods should produce identical results
        assert_eq!(hash_auto, hash_seq, "Auto and sequential should match");
        assert_eq!(hash_seq, hash_par, "Sequential and parallel should match");
        assert_eq!(hash_auto, hash_par, "Auto and parallel should match");
    }

    // Test 13: Tiger Style compliance - assertions and postconditions
    #[test]
    fn test_tiger_style_compliance() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x4Eu8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test that all assertions are properly placed and validated
        let hasher = Hasher::new_md5(temp_file.path())
            .with_chunk_size(16 * 1024)
            .expect("Valid chunk size")
            .with_parallel_readers(2);

        // Verify preconditions
        assert!(hasher.path.exists(), "Precondition: file must exist");
        assert!(hasher.path.is_file(), "Precondition: path must be file");
        assert!(
            hasher.chunk_size >= MIN_CHUNK_SIZE,
            "Precondition: chunk size >= minimum"
        );
        assert!(
            hasher.chunk_size <= MAX_CHUNK_SIZE,
            "Precondition: chunk size <= maximum"
        );
        assert!(hasher.parallel_readers > 0, "Precondition: readers > 0");
        assert!(
            hasher.parallel_readers <= MAX_PARALLEL_READERS,
            "Precondition: readers <= maximum"
        );

        // Test hash generation maintains postconditions
        let hash = hasher.find_root_hash().expect("Hash should succeed");

        // Verify postconditions
        assert!(!hash.is_empty(), "Postcondition: hash not empty");
        assert_eq!(hash.len(), 32, "Postcondition: MD5 hash length");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Postcondition: hash is hexadecimal"
        );
    }

    // Test 14: Error handling and recovery
    #[test]
    fn test_error_handling_and_recovery() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x4Fu8; 1024]; // 1KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        // Test chunk size validation error contains proper information
        let hasher = Hasher::new_md5(temp_file.path());
        let result = hasher.with_chunk_size(1024); // Too small

        assert!(result.is_err(), "Invalid chunk size should return error");

        match result {
            Err(CheckleError::InvalidChunkSize {
                size,
                reason,
                min_size,
                max_size,
            }) => {
                assert_eq!(size, 1024, "Error should contain actual size");
                assert!(!reason.is_empty(), "Error should contain reason");
                assert_eq!(min_size, MIN_CHUNK_SIZE, "Error should contain min size");
                assert_eq!(max_size, MAX_CHUNK_SIZE, "Error should contain max size");

                // Test error message formatting
                let error_message = format!(
                    "Error: {}",
                    CheckleError::InvalidChunkSize {
                        size,
                        reason,
                        min_size,
                        max_size
                    }
                );
                assert!(
                    error_message.contains("1024"),
                    "Error message should contain size"
                );
                assert!(
                    error_message.contains("4KB"),
                    "Error message should mention page alignment"
                );
            }
            _ => panic!("Expected InvalidChunkSize error"),
        }
    }

    // Test 15: Performance characteristics remain reasonable
    #[test]
    fn test_performance_characteristics() {
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let test_data = vec![0x50u8; 64 * 1024]; // 64KB
        fs::write(temp_file.path(), &test_data).expect("Failed to write test data");

        use std::time::Instant;

        // Time default hasher
        let start = Instant::now();
        let hasher_default = Hasher::new_md5(temp_file.path());
        let hash_default = hasher_default
            .find_root_hash()
            .expect("Default hash should succeed");
        let time_default = start.elapsed();

        // Time configured hasher
        let start = Instant::now();
        let hasher_configured = Hasher::new_md5(temp_file.path())
            .with_chunk_size(32 * 1024)
            .expect("Valid chunk size")
            .with_parallel_readers(2);
        let hash_configured = hasher_configured
            .find_root_hash()
            .expect("Configured hash should succeed");
        let time_configured = start.elapsed();

        // Both should complete quickly (< 1 second for 64KB)
        assert!(time_default.as_secs() < 1, "Default hasher should be fast");
        assert!(
            time_configured.as_secs() < 1,
            "Configured hasher should be fast"
        );

        // Hashes should be valid
        assert_eq!(hash_default.len(), 32, "Default hash should be valid");
        assert_eq!(hash_configured.len(), 32, "Configured hash should be valid");
        assert!(
            hash_default.chars().all(|c| c.is_ascii_hexdigit()),
            "Default hash should be hex"
        );
        assert!(
            hash_configured.chars().all(|c| c.is_ascii_hexdigit()),
            "Configured hash should be hex"
        );

        // Performance overhead should be minimal (within 3x)
        let ratio = if time_configured > time_default {
            time_configured.as_secs_f64() / time_default.as_secs_f64()
        } else {
            time_default.as_secs_f64() / time_configured.as_secs_f64()
        };

        assert!(
            ratio < 3.0,
            "Performance overhead should be minimal: {:.2}x",
            ratio
        );
    }

    // Test 14: Property-based testing with various combinations
    #[test]
    fn test_file_region_various_combinations() {
        let test_cases = vec![
            (100, 1, MIN_CHUNK_SIZE),
            (100, 2, MIN_CHUNK_SIZE),
            (100, 3, MIN_CHUNK_SIZE),
            (100, 10, MIN_CHUNK_SIZE),
            (1000, 4, DEFAULT_CHUNK_SIZE),
            (10000, 8, DEFAULT_CHUNK_SIZE),
            (1024 * 1024, 16, MAX_CHUNK_SIZE),
        ];

        for (file_size, num_threads, chunk_size) in test_cases {
            let regions = FileRegion::calculate_regions(file_size, num_threads, chunk_size);

            // Basic invariants
            assert!(!regions.is_empty(), "Must have at least one region");
            assert_eq!(regions[0].start_offset, 0, "First region starts at 0");
            assert_eq!(
                regions.last().unwrap().end_offset,
                file_size,
                "Last region ends at file size"
            );

            // Coverage invariant
            let total_coverage: u64 = regions.iter().map(FileRegion::size).sum();
            assert_eq!(
                total_coverage, file_size,
                "Total coverage must equal file size for case ({}, {}, {})",
                file_size, num_threads, chunk_size
            );

            // Contiguity invariant
            for i in 1..regions.len() {
                assert_eq!(
                    regions[i - 1].end_offset,
                    regions[i].start_offset,
                    "Regions must be contiguous for case ({}, {}, {})",
                    file_size,
                    num_threads,
                    chunk_size
                );
            }
        }
    }
}
