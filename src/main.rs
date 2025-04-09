#![warn(
    clippy::pedantic,
    clippy::perf,
    clippy::expect_used,
    clippy::todo,
    // missing_docs
)]
#![forbid(clippy::unwrap_used)]

use checkle::{
    cli::{self, Cli, Commands},
    io::{collect_files, FileHashPair, FilesToCheck},
    prelude::*,
};
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use color_eyre::{eyre::Context, Result};
use fern::colors::{Color, ColoredLevelConfig};
use jiff::Timestamp;
use log::debug;
use rayon::{
    iter::{IntoParallelIterator, ParallelIterator},
    ThreadPoolBuilder,
};
use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::Once,
};

fn main() -> Result<()> {
    // Parse provided command line arguments
    let cli = Cli::parse();

    // Determine how much verbosity the user requested and use that level to set up logging
    let verbosity = cli.verbose;
    setup_logger(&verbosity)?;

    // get the desired number of threads and hashing algorithm
    let thread_count = cli.threads;
    let algo = cli.algorithm;

    // set up the threadpool
    init_global_thread_pool(thread_count);

    match cli.command {
        // if no subcommand is provided in the command-line, just print the tool's info.
        None => {
            eprintln!("{}\n", cli::INFO);
            std::process::exit(0);
        }

        // Verify a single file with a single pre-computed hash.
        Some(Commands::Verify { input_file, hash }) => {
            match algo {
                HashingAlgo::Md5 => {
                    let hasher = Hasher::new_md5(&input_file);
                    hasher.checksum(&hash)?;
                }
                HashingAlgo::Sha2 => {
                    let hasher = Hasher::new_sha2(&input_file);
                    hasher.checksum(&hash)?;
                }
            }
            Ok(())
        }

        // Verify many files from a list of pre-computed hashes.
        Some(Commands::VerifyMany { checksum_file }) => {
            let files_to_check = FilesToCheck::new_from_txt(&checksum_file)?;
            files_to_check.checksum_all(&algo)?;
            Ok(())
        }

        // Generate a hash for one or more input files to be used when checksumming later.
        Some(Commands::Hash { input_file }) => {
            let files = collect_files(&input_file);
            let file_hash_pairs = files
                .into_par_iter()
                .map(|file| -> Result<FileHashPair> {
                    match algo {
                        HashingAlgo::Md5 => {
                            let hasher = Hasher::new_md5(&file);
                            let hash = hasher.find_root_hash()?;
                            Ok(FileHashPair::new(file, hash))
                        }
                        HashingAlgo::Sha2 => {
                            let hasher = Hasher::new_sha2(&file);
                            let hash = hasher.find_root_hash()?;
                            Ok(FileHashPair::new(file, hash))
                        }
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            debug!("Finished hashing {} file(s).", file_hash_pairs.len());

            let checksum_file = File::create("checksum.txt")?;
            let mut writer = BufWriter::new(checksum_file);
            for file in file_hash_pairs {
                debug!("Writing hash information for file {file:?}...");
                let (file, hash) = file.file_hash_owned();
                let line = format!("{hash}\t{}", file.to_string_lossy().clone());
                writer.write_all(line.as_bytes())?;
            }
            writer.flush()?;
            Ok(())
        }
    }
}

fn setup_logger(verbosity: &Verbosity) -> Result<()> {
    // set up the logging verbosity as provided by the user
    let level = verbosity.log_level_filter();

    // set up color eyre
    color_eyre::install()?;

    // set colors for the logs based on their level, because why not
    let colors = ColoredLevelConfig::new()
        .trace(Color::BrightBlue)
        .debug(Color::Blue)
        .warn(Color::Yellow)
        .error(Color::Red)
        .info(Color::Green);

    // build and apply a new logger instance user fern and the user's desired verbosity
    fern::Dispatch::new()
        .level(level)
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("clap", log::LevelFilter::Warn)
        .level_for("clap_builder", log::LevelFilter::Warn)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                Timestamp::now(),
                colors.color(record.level()),
                record.target(),
                message,
            ));
        })
        .chain(std::io::stderr())
        .apply()
        .with_context(|| "Failed to setup logging.")?;

    Ok(())
}

// Use a Once to ensure initialization happens exactly once
static INIT: Once = Once::new();

// Initialize the global Rayon thread pool
#[allow(clippy::expect_used)]
fn init_global_thread_pool(num_threads: Option<usize>) {
    let num_threads = if let Some(threads) = num_threads {
        threads
    } else {
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(4)
    };

    // Globally initialize a threadpool
    INIT.call_once(|| {
        ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build_global()
            .expect("Failed to build global thread pool");

        debug!("Global thread pool initialized with {num_threads} threads");
    });
}
