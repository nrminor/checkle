#![warn(
    clippy::pedantic,
    clippy::perf,
    clippy::expect_used,
    clippy::todo,
    // missing_docs
)]
// #![forbid(clippy::unwrap_used)]
#![crate_name = "checkle"]

pub mod cli;
pub mod hashing;
pub mod io;
pub mod prelude;
