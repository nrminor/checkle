//! The checkle prelude - commonly used types for library consumers.
//!
//! This module re-exports the most commonly used types and traits from checkle,
//! allowing library users to import them all at once with:
//!
//! ```
//! use checkle::prelude::*;
//! ```

pub use crate::{
    // Archive path parsing
    archive_path::{ArchivePathComponents, parse_archive_path},
    // Common constants that library users might need
    constants::{
        CHUNK_SIZE, DEFAULT_CHUNK_SIZE, MAX_CHUNK_SIZE, MAX_FILES_IN_BATCH, MAX_PARALLEL_READERS,
        MIN_CHUNK_SIZE, MIN_FILE_SIZE_FOR_PROGRESS, PROGRESS_VISIBILITY_THRESHOLD,
    },
    // Data source abstraction
    data_source::DataSource,

    // Error handling
    errors::{CheckleError, Result},

    // Core hashing functionality
    hashing::{HashArray, Hasher, HashingAlgo, MerkleIter},

    // I/O operations
    io::{FileHashPair, FilesToCheck},

    // Progress tracking
    progress::{FileProgress, ProgressManager},
};
