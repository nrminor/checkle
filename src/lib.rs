//! # ⚠️ CRITICAL: DO NOT USE FOR STANDARD CHECKSUMS
//!
//! This project is an unsuccessful prototype that will produce different hashes
//! than `md5sum` and `sha256sum` for all files larger than 1MB. `checkle` is thus
//! incompatible with standard MD5/SHA256 checksum utilities.
//!
//! Please use standard time-tested tools like `md5sum` or `sha256sum` instead.
//!
//! ---
//!
//! # checkle
//!
//! A high-performance checksum utility using Merkle tree parallelization.
//!
//! **Note**: While checkle produces deterministic hashes and can verify file integrity
//! when used on both endpoints, it does NOT produce MD5 or SHA256 compatible hashes
//! for files larger than 1MB due to its Merkle tree implementation.

#![crate_name = "checkle"]
#![cfg_attr(feature = "simd", feature(portable_simd))]

#[cfg(any(feature = "tar", feature = "zip"))]
pub mod archive;
pub mod archive_path;
pub mod buffer_pool;
pub mod cli;
pub mod constants;
pub mod data_source;
pub mod errors;
pub mod hashing;
pub mod io;
pub mod prelude;
pub mod prettyprint;
pub mod progress;
pub mod simd;

pub mod commands;
