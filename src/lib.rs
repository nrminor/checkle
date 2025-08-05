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
