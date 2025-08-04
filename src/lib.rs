#![warn(clippy::pedantic, clippy::perf, clippy::expect_used, clippy::todo)]
#![crate_name = "checkle"]

#[cfg(any(feature = "tar", feature = "zip"))]
pub mod archive;
pub mod archive_path;
pub mod buffer_pool;
pub mod cli;
pub mod constants;
pub mod data_source;
pub mod data_source_hasher;
pub mod errors;
pub mod hashing;
pub mod io;
pub mod prelude;
pub mod prettyprint;
pub mod progress;
