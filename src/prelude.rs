pub use crate::hashing::{Hasher, HashingAlgo, MerkleIter};
pub use errors::*;

pub mod errors {
    use std::path::PathBuf;

    use thiserror::Error;

    /// An enum representing all the possible errors that can occur while computing, saving,
    /// and verifying checksums of files.
    ///
    /// This handles common issues like failed checksums due to file corruption,
    /// inaccessible files, and invalid checksum file formats.
    ///
    /// # Errors
    ///
    /// Returns errors when:
    /// - Files fail checksum verification
    /// - Files are inaccessible or don't exist
    /// - Checksum files are improperly formatted
    /// - Multiple files fail checksum verification in a batch
    ///
    /// # Panics
    ///
    /// This error type itself does not panic.
    #[derive(Debug, Error)]
    pub enum CheckleError {
        #[error("The provided file `{0}` failed the checksum process. It was likely truncated during a file transfer or otherwise mutated since the hash was originally computed.")]
        FailedChecksum(PathBuf),
        #[error("Multiple files failed the checksum. See logged output above.")]
        MultipleFailedChecksums,
        #[error("The provided file `{0}` does not exist or is otherwise inaccessible.")]
        InaccessibleFile(PathBuf),
        #[error("The provided checksum file `{0}` was invalid and could not be parsed. Please double check that it is tab delimited with two columns and no header, where the first column is the hash and the second column is the corresponding file path (relative or absolute).")]
        InvalidChecksumFile(PathBuf),
        #[error("Unknown error encountered.")]
        UnknownError(#[from] color_eyre::Report),
    }
    pub use CheckleError::*;
}
