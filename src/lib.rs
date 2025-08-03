#![warn(clippy::pedantic, clippy::perf, clippy::expect_used, clippy::todo)]
#![crate_name = "checkle"]

pub mod buffer_pool;
pub mod cli;
pub mod constants;
pub mod errors;
pub mod hashing;
pub mod io;
pub mod prelude;
pub mod prettyprint;
pub mod progress;
