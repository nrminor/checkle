#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::perf,
    clippy::style,
    clippy::complexity,
    clippy::correctness,
    clippy::unwrap_used
)]

use checkle::{
    cli::{self, Cli, Commands},
    io::{FilesToCheck, collect_files},
    prelude::*,
    prettyprint::{
        FileHashPairWithMetadata, VerificationResult, VerificationStatus, convert_to_basic_pairs,
        display_pretty_table, display_verification_table, display_verification_table_with_summary,
    },
    progress::ProgressManager,
};
use clap::Parser;
use color_eyre::Result;
use log::{debug, error, info};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::{
    io::{BufWriter, Write},
    path::PathBuf,
};

#[allow(clippy::too_many_lines)]
fn main() -> Result<()> {
    // Parse provided command line arguments
    let cli = Cli::parse();

    // Set up logging, run preflight checks, and initialize thread pool
    preflight::setup(&cli.verbose, cli.threads)?;
    preflight::checks();

    // get the hashing algorithm
    let algo = &cli.algorithm;

    match cli.command {
        // if no subcommand is provided in the command-line, just print the tool's info.
        None => {
            eprintln!("{}\n", cli::INFO);
            std::process::exit(0);
        }

        // Verify a single file with a single pre-computed hash.
        Some(Commands::Verify {
            ref input_file,
            ref hash,
            per_file,
            pretty,
        }) => {
            let hash_to_verify = if per_file {
                // Read hash from per-file hash file
                utils::read_per_file_hash(input_file, algo)?
            } else {
                // Use provided hash
                hash.as_ref()
                    .ok_or_else(|| {
                        CheckleError::InvalidChecksumFile(PathBuf::from("command line"))
                    })?
                    .clone()
            };

            // Collect verification result for pretty printing
            if pretty {
                // Check if file exists first
                if !input_file.exists() {
                    let result =
                        VerificationResult::new_missing(input_file.clone(), hash_to_verify.clone());
                    display_verification_table(&[result])?;
                    return Err(CheckleError::InaccessibleFile(input_file.clone()).into());
                }

                // Perform verification and collect result
                let computed_hash = match *algo {
                    HashingAlgo::Md5 => {
                        let hasher = Hasher::new_md5(input_file);
                        let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                        match configured_hasher.find_root_hash() {
                            Ok(hash) => hash,
                            Err(e) => {
                                let result = VerificationResult::new_error(
                                    input_file.clone(),
                                    hash_to_verify.clone(),
                                    e.to_string(),
                                );
                                display_verification_table(&[result])?;
                                return Err(e.into());
                            }
                        }
                    }
                    HashingAlgo::Sha2 => {
                        let hasher = Hasher::new_sha2(input_file);
                        let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                        match configured_hasher.find_root_hash() {
                            Ok(hash) => hash,
                            Err(e) => {
                                let result = VerificationResult::new_error(
                                    input_file.clone(),
                                    hash_to_verify.clone(),
                                    e.to_string(),
                                );
                                display_verification_table(&[result])?;
                                return Err(e.into());
                            }
                        }
                    }
                };

                let passed = computed_hash == hash_to_verify;

                // Get file metadata for display
                let result = if let Ok(metadata) = std::fs::metadata(input_file) {
                    VerificationResult::new_with_metadata(
                        input_file.clone(),
                        hash_to_verify.clone(),
                        computed_hash,
                        passed,
                        &metadata,
                    )
                } else {
                    VerificationResult::new(
                        input_file.clone(),
                        hash_to_verify.clone(),
                        computed_hash,
                        passed,
                    )
                };

                display_verification_table(&[result.clone()])?;

                if !passed {
                    return Err(CheckleError::FailedChecksum(input_file.clone()).into());
                }

                Ok(())
            } else {
                // Original non-pretty logic
                match *algo {
                    HashingAlgo::Md5 => {
                        let hasher = Hasher::new_md5(input_file);
                        let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                        configured_hasher.checksum(&hash_to_verify)?;
                    }
                    HashingAlgo::Sha2 => {
                        let hasher = Hasher::new_sha2(input_file);
                        let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                        configured_hasher.checksum(&hash_to_verify)?;
                    }
                }
                Ok(())
            }
        }

        // Verify many files from a list of pre-computed hashes.
        Some(Commands::VerifyMany {
            ref checksum_file,
            per_file,
            ref files,
            pretty,
            ref report,
            format,
        }) => {
            if per_file {
                // Verify each file using its per-file hash
                let mut verification_results = Vec::new();

                for file_path in files {
                    let verification_result = match utils::read_per_file_hash(file_path, algo) {
                        Ok(expected_hash) => {
                            if file_path.exists() {
                                let computed_hash_result = match *algo {
                                    HashingAlgo::Md5 => {
                                        let hasher = Hasher::new_md5(file_path);
                                        let configured_hasher =
                                            utils::configure_hasher(hasher, &cli)?;
                                        configured_hasher.find_root_hash()
                                    }
                                    HashingAlgo::Sha2 => {
                                        let hasher = Hasher::new_sha2(file_path);
                                        let configured_hasher =
                                            utils::configure_hasher(hasher, &cli)?;
                                        configured_hasher.find_root_hash()
                                    }
                                };

                                match computed_hash_result {
                                    Ok(computed_hash) => {
                                        let passed = computed_hash == expected_hash;
                                        if let Ok(metadata) = std::fs::metadata(file_path) {
                                            VerificationResult::new_with_metadata(
                                                file_path.clone(),
                                                expected_hash,
                                                computed_hash,
                                                passed,
                                                &metadata,
                                            )
                                        } else {
                                            VerificationResult::new(
                                                file_path.clone(),
                                                expected_hash,
                                                computed_hash,
                                                passed,
                                            )
                                        }
                                    }
                                    Err(e) => VerificationResult::new_error(
                                        file_path.clone(),
                                        expected_hash,
                                        e.to_string(),
                                    ),
                                }
                            } else {
                                VerificationResult::new_missing(file_path.clone(), expected_hash)
                            }
                        }
                        Err(e) => VerificationResult::new_error(
                            file_path.clone(),
                            String::new(),
                            format!("Cannot read hash file: {e}"),
                        ),
                    };

                    verification_results.push(verification_result);
                }

                // Handle output based on flags
                if report.is_some() || format.is_some() {
                    // Use report functionality - detect format from file extension if not explicitly provided
                    let output_format = if let Some(fmt) = format {
                        // Explicit format provided via --format flag
                        fmt
                    } else if let Some(report_path) = &report {
                        // Auto-detect from file extension
                        cli::OutputFormat::detect_from_path(report_path)
                    } else {
                        // Default to text format
                        cli::OutputFormat::Text
                    };
                    let formatted_output = utils::format_verification_output_with_pretty(
                        &verification_results,
                        output_format,
                        pretty,
                    );

                    if let Some(report_path) = report {
                        // Write to file
                        std::fs::write(report_path, formatted_output)?;
                    } else {
                        // Write to stdout
                        println!("{formatted_output}");
                    }
                } else if pretty {
                    display_verification_table_with_summary(&verification_results)?;
                } else {
                    // Structured verification output - tab-delimited format like hash command
                    // Collect all output lines first to print as a single block (avoids interleaving with logs)
                    let mut output_lines = Vec::new();
                    let mut failed_files = Vec::new();
                    let mut successful_count = 0;

                    for result in &verification_results {
                        match result.status() {
                            VerificationStatus::Pass => {
                                output_lines.push(format!("PASS\t{}", result.file().display()));
                                info!("Verified: {}", result.file().display());
                                successful_count += 1;
                            }
                            VerificationStatus::Fail => {
                                output_lines.push(format!("FAIL\t{}", result.file().display()));
                                error!(
                                    "Verification failed for {}: {}",
                                    result.file().display(),
                                    result.error_message().unwrap_or("Hash mismatch")
                                );
                                failed_files.push(result.file().to_path_buf());
                            }
                            VerificationStatus::Missing => {
                                output_lines.push(format!("MISS\t{}", result.file().display()));
                                error!("File not found: {}", result.file().display());
                                failed_files.push(result.file().to_path_buf());
                            }
                            VerificationStatus::Error(_) => {
                                output_lines.push(format!("ERROR\t{}", result.file().display()));
                                error!(
                                    "Error verifying {}: {}",
                                    result.file().display(),
                                    result.error_message().unwrap_or("Unknown error")
                                );
                                failed_files.push(result.file().to_path_buf());
                            }
                        }
                    }

                    // Print all verification results as a single block to stdout
                    if !output_lines.is_empty() {
                        println!("{}", output_lines.join("\n"));
                    }

                    info!("Verified {successful_count} files successfully");

                    if !failed_files.is_empty() {
                        error!("{} files failed verification", failed_files.len());
                        return Err(CheckleError::MultipleFailedChecksums.into());
                    }
                }

                // Check if any verifications failed
                let has_failures = verification_results.iter().any(|r| !r.passed());
                if has_failures {
                    return Err(CheckleError::MultipleFailedChecksums.into());
                }
            } else {
                // Use checksum file
                let checksum_file_path = checksum_file.as_ref().ok_or_else(|| {
                    CheckleError::InvalidChecksumFile(PathBuf::from("command line"))
                })?;

                if pretty || report.is_some() || format.is_some() {
                    // Parse the checksum file including missing files for complete verification reporting
                    let file_hash_pairs = utils::parse_checksum_file_raw(checksum_file_path)?;
                    let mut verification_results = Vec::new();

                    for (file_path, expected_hash) in file_hash_pairs {
                        let verification_result = if file_path.exists() {
                            let computed_hash_result = match *algo {
                                HashingAlgo::Md5 => {
                                    let hasher = Hasher::new_md5(&file_path);
                                    let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                                    configured_hasher.find_root_hash()
                                }
                                HashingAlgo::Sha2 => {
                                    let hasher = Hasher::new_sha2(&file_path);
                                    let configured_hasher = utils::configure_hasher(hasher, &cli)?;
                                    configured_hasher.find_root_hash()
                                }
                            };

                            match computed_hash_result {
                                Ok(computed_hash) => {
                                    let passed = computed_hash == expected_hash;
                                    if let Ok(metadata) = std::fs::metadata(&file_path) {
                                        VerificationResult::new_with_metadata(
                                            file_path.clone(),
                                            expected_hash,
                                            computed_hash,
                                            passed,
                                            &metadata,
                                        )
                                    } else {
                                        VerificationResult::new(
                                            file_path.clone(),
                                            expected_hash,
                                            computed_hash,
                                            passed,
                                        )
                                    }
                                }
                                Err(e) => VerificationResult::new_error(
                                    file_path.clone(),
                                    expected_hash,
                                    e.to_string(),
                                ),
                            }
                        } else {
                            VerificationResult::new_missing(file_path.clone(), expected_hash)
                        };

                        verification_results.push(verification_result);
                    }

                    // Handle output based on flags
                    if report.is_some() || format.is_some() {
                        // Use report functionality - detect format from file extension if not explicitly provided
                        let output_format = if let Some(fmt) = format {
                            // Explicit format provided via --format flag
                            fmt
                        } else if let Some(report_path) = &report {
                            // Auto-detect from file extension
                            cli::OutputFormat::detect_from_path(report_path)
                        } else {
                            // Default to text format
                            cli::OutputFormat::Text
                        };
                        let formatted_output = utils::format_verification_output_with_pretty(
                            &verification_results,
                            output_format,
                            pretty,
                        );

                        if let Some(report_path) = report {
                            // Write to file
                            std::fs::write(report_path, formatted_output)?;
                        } else {
                            // Write to stdout
                            println!("{formatted_output}");
                        }
                    } else if pretty {
                        display_verification_table_with_summary(&verification_results)?;
                    }

                    // Check if any verifications failed
                    let has_failures = verification_results.iter().any(|r| !r.passed());
                    if has_failures {
                        return Err(CheckleError::MultipleFailedChecksums.into());
                    }
                } else {
                    // Original checksum_all logic
                    let files_to_check = FilesToCheck::new_from_txt(checksum_file_path)?;
                    files_to_check.checksum_all(algo)?;
                }
            }
            Ok(())
        }

        // Generate a hash for one or more input files to be used when checksumming later.
        Some(Commands::Hash {
            ref input_file,
            recursive,
            ref hash_output,
            format,
            pretty,
            per_file,
            no_progress,
        }) => {
            let files = collect_files(input_file, recursive)?;

            // Create progress manager based on no_progress flag
            // Show progress unless explicitly disabled
            let show_progress = !no_progress;
            let progress_manager = ProgressManager::new(show_progress, files.len());

            // Clone necessary values for use in parallel iterator
            let progress_manager_clone = progress_manager.clone();
            let chunk_size_kb = cli.chunk_size_kb;
            let parallel_readers = cli.parallel_readers;

            let file_hash_pairs = files
                .into_par_iter()
                .map(move |file| -> Result<FileHashPairWithMetadata> {
                    // Get file metadata for both progress tracking and pretty output
                    let (file_size, metadata_opt) = match std::fs::metadata(&file) {
                        Ok(metadata) => (metadata.len(), Some(metadata)),
                        Err(_) => (0, None), // If we can't get metadata, use fallback
                    };

                    // Create per-file progress bar if the file is large enough
                    let file_progress = progress_manager_clone
                        .create_file_progress(file.to_string_lossy().as_ref(), file_size);

                    let result = match *algo {
                        HashingAlgo::Md5 => {
                            let mut hasher = Hasher::new_md5(&file);

                            // Configure hasher with cloned values
                            hasher = hasher.with_chunk_size(chunk_size_kb * 1024)?;
                            if parallel_readers > 0 {
                                hasher = hasher.with_parallel_readers(parallel_readers);
                            }

                            // Add progress callback if we have a progress bar
                            if let Some(progress) = file_progress {
                                hasher =
                                    hasher.with_progress_callback(Box::new(move |bytes_read| {
                                        progress.update(bytes_read);
                                    }));
                            }

                            let hash = hasher.find_root_hash()?;
                            Ok(match metadata_opt {
                                Some(metadata) => {
                                    FileHashPairWithMetadata::new(file, hash, &metadata)
                                }
                                None => FileHashPairWithMetadata::new_with_fallback(file, hash),
                            })
                        }
                        HashingAlgo::Sha2 => {
                            let mut hasher = Hasher::new_sha2(&file);

                            // Configure hasher with cloned values
                            hasher = hasher.with_chunk_size(chunk_size_kb * 1024)?;
                            if parallel_readers > 0 {
                                hasher = hasher.with_parallel_readers(parallel_readers);
                            }

                            // Add progress callback if we have a progress bar
                            if let Some(progress) = file_progress {
                                hasher =
                                    hasher.with_progress_callback(Box::new(move |bytes_read| {
                                        progress.update(bytes_read);
                                    }));
                            }

                            let hash = hasher.find_root_hash()?;
                            Ok(match metadata_opt {
                                Some(metadata) => {
                                    FileHashPairWithMetadata::new(file, hash, &metadata)
                                }
                                None => FileHashPairWithMetadata::new_with_fallback(file, hash),
                            })
                        }
                    };

                    // Update overall progress
                    progress_manager_clone.inc_overall();

                    result
                })
                .collect::<Result<Vec<_>>>()?;

            // Finish progress display
            progress_manager
                .finish_with_message(&format!("Hashed {} files", file_hash_pairs.len()));

            debug!("Finished hashing {} file(s).", file_hash_pairs.len());

            // Display pretty table to stderr if requested
            if pretty {
                display_pretty_table(&file_hash_pairs)?;
            }

            // Convert enhanced pairs to basic pairs for backward compatibility
            let basic_file_hash_pairs = convert_to_basic_pairs(file_hash_pairs);

            // Determine output format
            let output_format = if let Some(fmt) = format {
                // Explicit format provided via --format flag
                fmt
            } else if let Some(output_path) = &hash_output {
                // Auto-detect from file extension
                cli::OutputFormat::detect_from_path(output_path)
            } else {
                // Default to text format
                cli::OutputFormat::Text
            };

            // Handle file output and optional stdout output
            if per_file {
                // Write hash files alongside each source file
                for file_hash_pair in &basic_file_hash_pairs {
                    utils::write_per_file_hash(file_hash_pair.file(), file_hash_pair.hash(), algo)?;
                }

                info!("Created {} hash files", basic_file_hash_pairs.len());

                // Still output to stdout for visibility unless pretty mode
                if !pretty {
                    println!(
                        "{}",
                        utils::format_output_with_pretty(
                            &basic_file_hash_pairs,
                            output_format,
                            pretty
                        )
                    );
                }
            } else if let Some(output_path) = hash_output {
                // Write to specified file
                let checksum_file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output_path)
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::AlreadyExists {
                            CheckleError::OutputFileExists {
                                path: output_path.clone(),
                            }
                        } else {
                            CheckleError::FileOpenError {
                                path: output_path.clone(),
                                source: e,
                            }
                        }
                    })?;

                let mut writer = BufWriter::new(checksum_file);
                let formatted_output =
                    utils::format_output_with_pretty(&basic_file_hash_pairs, output_format, pretty);
                debug!(
                    "Writing {} formatted hash records...",
                    basic_file_hash_pairs.len()
                );
                write!(writer, "{formatted_output}")?;
                writer.flush()?;

                info!("Hashes written to: {}", output_path.display());

                // Print to stdout unless pretty mode is enabled (which outputs to stderr instead)
                if !pretty {
                    println!(
                        "{}",
                        utils::format_output_with_pretty(
                            &basic_file_hash_pairs,
                            output_format,
                            pretty
                        )
                    );
                }
            } else {
                // Default behavior: output only to stdout (no file creation)
                let formatted_output =
                    utils::format_output_with_pretty(&basic_file_hash_pairs, output_format, pretty);
                debug!(
                    "Writing {} formatted hash records to stdout...",
                    basic_file_hash_pairs.len()
                );

                // Print to stdout unless pretty mode is enabled (which outputs to stderr instead)
                if !pretty {
                    println!("{formatted_output}");
                }
            }
            Ok(())
        }
    }
}

mod preflight {
    use clap_verbosity_flag::{Verbosity, WarnLevel};
    use color_eyre::{Result, eyre::Context};
    use fern::colors::{Color, ColoredLevelConfig};
    use jiff::Timestamp;
    use log::{debug, info, warn};
    use rayon::ThreadPoolBuilder;
    use std::{env, io::Write, num::NonZeroUsize, sync::Once, thread};

    // Use a Once to ensure initialization happens exactly once
    static INIT: Once = Once::new();

    pub(super) fn setup(
        verbosity: &Verbosity<WarnLevel>,
        thread_count: Option<usize>,
    ) -> Result<()> {
        // Set up logging first
        setup_logger(verbosity)?;

        // Initialize the thread pool
        init_global_thread_pool(thread_count);

        Ok(())
    }

    pub(super) fn checks() {
        rayon::scope(|s| {
            s.spawn(|_| check_cpu_cores());
            s.spawn(|_| check_storage_type());
            s.spawn(|_| check_temp_directory());
        });
    }

    fn setup_logger(verbosity: &Verbosity<WarnLevel>) -> Result<()> {
        // set up the logging verbosity as provided by the user
        let level = verbosity.log_level_filter();

        // Configure backtrace based on verbosity
        // Only show backtraces for debug (-vvv) and trace (-vvvv) levels
        if std::env::var("RUST_LIB_BACKTRACE").is_err() {
            let should_show_backtrace =
                matches!(level, log::LevelFilter::Debug | log::LevelFilter::Trace);

            #[allow(clippy::disallowed_methods)] // Setting env var for error handling is required
            unsafe {
                std::env::set_var(
                    "RUST_LIB_BACKTRACE",
                    if should_show_backtrace { "1" } else { "0" },
                );
            }
        }

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

    // Initialize the global Rayon thread pool
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
            if let Err(source) = ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build_global()
            {
                panic!("Failed to build global thread pool: {source}");
            }

            debug!("Global thread pool initialized with {num_threads} threads");
        });
    }

    fn check_cpu_cores() {
        let cores = thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1);

        info!("Detected {cores} CPU cores");

        if cores < 2 {
            warn!(
                "Only {cores} CPU core detected. This tool benefits significantly from multiple cores. \
                Consider running on a machine with more cores for better performance."
            );
        } else if cores < 4 {
            warn!(
                "Only {cores} CPU cores detected. This tool performs best with 4 or more cores. \
                Performance may be limited."
            );
        }
    }

    fn check_storage_type() {
        let is_likely_ssd = check_if_ssd();

        if is_likely_ssd {
            info!("Storage appears to be SSD (optimal for performance)");
        } else {
            warn!(
                "Storage may not be an SSD. This tool performs significantly better on SSDs \
                due to intensive I/O operations. Consider using SSD storage for optimal performance."
            );
        }
    }

    fn check_if_ssd() -> bool {
        #[cfg(target_os = "linux")]
        {
            check_linux_storage_type()
        }

        #[cfg(target_os = "macos")]
        {
            check_macos_storage_type()
        }

        #[cfg(target_os = "windows")]
        {
            check_windows_storage_type()
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            true
        }
    }

    #[cfg(target_os = "linux")]
    fn check_linux_storage_type() -> bool {
        use std::fs;

        for device in ["sda", "nvme0n1", "vda", "xvda"] {
            let rotational_path = format!("/sys/block/{}/queue/rotational", device);
            if let Ok(content) = fs::read_to_string(&rotational_path) {
                if content.trim() == "0" {
                    return true;
                }
            }
        }

        false
    }

    #[cfg(target_os = "macos")]
    fn check_macos_storage_type() -> bool {
        use std::process::Command;

        let output = Command::new("diskutil").args(["info", "/"]).output().ok();

        if let Some(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Check for various SSD indicators using more robust matching
            let normalized = output_str.replace('\t', " ");
            let lines: Vec<&str> = normalized.lines().collect();

            for line in lines {
                // Check if line contains "Solid State" and "Yes"
                if line.contains("Solid State") && line.contains("Yes") {
                    return true;
                }
                // Check for SSD media type
                if line.contains("Media Type") && line.contains("SSD") {
                    return true;
                }
                // Check for NVMe/PCI-Express protocol (typically indicates SSD)
                if line.contains("Protocol")
                    && (line.contains("PCI-Express") || line.contains("NVMe"))
                {
                    return true;
                }
            }
        }

        // On modern Macs, assume SSD if we can't determine
        true
    }

    #[cfg(target_os = "windows")]
    fn check_windows_storage_type() -> bool {
        use std::process::Command;

        let script = "Get-PhysicalDisk | Where-Object {$_.MediaType -eq 'SSD'} | Measure-Object | Select-Object -ExpandProperty Count";

        let output = Command::new("powershell")
            .args(&["-Command", script])
            .output()
            .ok();

        if let Some(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(count) = output_str.trim().parse::<u32>() {
                return count > 0;
            }
        }

        false
    }

    fn check_temp_directory() {
        use std::fs::File;
        let temp_dir = env::temp_dir();
        let test_file = temp_dir.join("checkle_preflight_test");

        let can_write = File::create(&test_file)
            .and_then(|mut f| f.write_all(b"test"))
            .and_then(|()| std::fs::remove_file(&test_file))
            .is_ok();

        if !can_write {
            warn!(
                "Cannot write to temporary directory. Some operations may fail. \
                Please ensure {} is writable.",
                temp_dir.display()
            );
        }
    }
}
mod utils {
    use checkle::{
        cli::{Cli, OutputFormat},
        errors::CheckleError,
        io::FileHashPair,
        prelude::{Hasher, HashingAlgo, Result},
        prettyprint::VerificationResult,
    };
    use log::debug;
    use std::{
        fmt::Write,
        fs,
        path::{Path, PathBuf},
    };

    // Helper function to format output with optional pretty printing
    pub(super) fn format_output_with_pretty(
        file_hash_pairs: &[FileHashPair],
        format: OutputFormat,
        pretty: bool,
    ) -> String {
        match format {
            OutputFormat::Text => {
                // Tab-delimited format: hash\tfile_path
                let lines: Vec<String> = file_hash_pairs
                    .iter()
                    .map(|file| format!("{}\t{}", file.hash(), file.file().to_string_lossy()))
                    .collect();
                lines.join("\n")
            }
            OutputFormat::Csv => {
                // CSV format with proper escaping
                let mut output = String::from("hash,filepath\n"); // Header
                for file in file_hash_pairs {
                    let hash = file.hash();
                    let filepath = file.file().to_string_lossy();

                    // Escape CSV fields if they contain special characters
                    let escaped_filepath = if filepath.contains([',', '"', '\n', '\r']) {
                        format!("\"{}\"", filepath.replace('"', "\"\""))
                    } else {
                        filepath.to_string()
                    };

                    writeln!(output, "{hash},{escaped_filepath}")
                        .expect("Writing to String should never fail");
                }
                output.trim_end().to_string() // Remove trailing newline
            }
            OutputFormat::Json => {
                // For now, create JSON manually to avoid adding serde dependency
                // This will be replaced with proper serde serialization
                if pretty {
                    format_hash_json_pretty(file_hash_pairs)
                } else {
                    format_hash_json_compact(file_hash_pairs)
                }
            }
        }
    }

    fn format_hash_json_compact(file_hash_pairs: &[FileHashPair]) -> String {
        let mut json_objects = Vec::new();
        for file in file_hash_pairs {
            let hash = file.hash();
            let filepath = file.file().to_string_lossy();
            let escaped_filepath = escape_json_string(&filepath);
            json_objects.push(format!(
                "{{\"hash\":\"{hash}\",\"filepath\":\"{escaped_filepath}\"}}"
            ));
        }
        format!("[{}]", json_objects.join(","))
    }

    fn format_hash_json_pretty(file_hash_pairs: &[FileHashPair]) -> String {
        let mut output = String::from("[\n");
        for (i, file) in file_hash_pairs.iter().enumerate() {
            let hash = file.hash();
            let filepath = file.file().to_string_lossy();
            let escaped_filepath = escape_json_string(&filepath);

            output.push_str("  {\n");
            writeln!(output, "    \"hash\": \"{hash}\",").unwrap();
            writeln!(output, "    \"filepath\": \"{escaped_filepath}\"").unwrap();

            if i == file_hash_pairs.len() - 1 {
                output.push_str("  }\n");
            } else {
                output.push_str("  },\n");
            }
        }
        output.push_str("]\n");
        output
    }

    // Helper function to configure hasher with CLI options
    pub(crate) fn configure_hasher<'a, const N: usize>(
        hasher: Hasher<'a, N>,
        cli: &Cli,
    ) -> Result<Hasher<'a, N>> {
        let mut configured_hasher = hasher;

        // Configure chunk size if different from default
        let chunk_size_bytes = cli.chunk_size_kb * 1024;
        configured_hasher = configured_hasher.with_chunk_size(chunk_size_bytes)?;

        // Configure parallel readers (0 = auto-detect)
        if cli.parallel_readers > 0 {
            configured_hasher = configured_hasher.with_parallel_readers(cli.parallel_readers);
        }

        // Log the configuration choices
        debug!(
            "Hasher configured with chunk size: {} KB",
            cli.chunk_size_kb
        );
        if cli.parallel_readers == 0 {
            debug!("Parallel readers: auto-detect");
        } else {
            debug!("Parallel readers: {}", cli.parallel_readers);
        }

        Ok(configured_hasher)
    }

    // Helper function to get the per-file hash filename
    pub(super) fn get_per_file_hash_path(file_path: &Path, algorithm: &HashingAlgo) -> PathBuf {
        let extension = match algorithm {
            HashingAlgo::Md5 => "md5",
            HashingAlgo::Sha2 => "sha256",
        };

        let mut hash_path = file_path.to_path_buf();
        let current_name = hash_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        let new_name = format!("{current_name}.{extension}");
        hash_path.set_file_name(new_name);
        hash_path
    }

    // Helper function to write hash to per-file hash file
    pub(super) fn write_per_file_hash(
        file_path: &Path,
        hash: &str,
        algorithm: &HashingAlgo,
    ) -> Result<()> {
        let hash_file_path = get_per_file_hash_path(file_path, algorithm);

        // Write in standard format: "hash  filename" (two spaces)
        // This matches the output format of md5sum and sha256sum tools
        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let content = format!("{hash}  {filename}\n");

        fs::write(&hash_file_path, content).map_err(|e| CheckleError::FileOpenError {
            path: hash_file_path.clone(),
            source: e,
        })?;

        debug!("Wrote hash to: {}", hash_file_path.display());
        Ok(())
    }

    // Helper function to read hash from per-file hash file
    pub(super) fn read_per_file_hash(file_path: &Path, algorithm: &HashingAlgo) -> Result<String> {
        let hash_file_path = get_per_file_hash_path(file_path, algorithm);

        if !hash_file_path.exists() {
            return Err(CheckleError::InaccessibleFile(hash_file_path));
        }

        let content =
            fs::read_to_string(&hash_file_path).map_err(|e| CheckleError::FileReadError {
                path: hash_file_path.clone(),
                source: e,
            })?;

        // Extract the hash from the first line
        // Support both formats:
        // 1. Just the hash: "d41d8cd98f00b204e9800998ecf8427e"
        // 2. Hash with filename: "d41d8cd98f00b204e9800998ecf8427e  filename.txt"
        let first_line = content
            .lines()
            .next()
            .ok_or_else(|| CheckleError::InvalidChecksumFile(hash_file_path.clone()))?
            .trim();

        // If the line contains whitespace, assume it's in "hash filename" format
        let hash = if first_line.contains(char::is_whitespace) {
            first_line
                .split_whitespace()
                .next()
                .ok_or_else(|| CheckleError::InvalidChecksumFile(hash_file_path.clone()))?
                .to_string()
        } else {
            first_line.to_string()
        };

        Ok(hash)
    }

    // Helper function to parse checksum file including missing files
    pub(super) fn parse_checksum_file_raw(checksum_file: &Path) -> Result<Vec<(PathBuf, String)>> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file_handle = File::open(checksum_file)
            .map_err(|_| CheckleError::InaccessibleFile(checksum_file.to_path_buf()))?;
        let buffer = BufReader::new(file_handle);

        let mut pairs = Vec::new();

        for line in buffer.lines() {
            let line =
                line.map_err(|_| CheckleError::InvalidChecksumFile(checksum_file.to_path_buf()))?;
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            let mut fields = line.split('\t');
            let Some(hash) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };
            let Some(file_str) = fields.next() else {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            };

            // Ensure no extra fields remain
            if fields.next().is_some() {
                return Err(CheckleError::InvalidChecksumFile(
                    checksum_file.to_path_buf(),
                ));
            }

            pairs.push((PathBuf::from(file_str), hash.to_string()));
        }

        Ok(pairs)
    }

    // Helper function to format verification results with optional pretty printing
    pub(super) fn format_verification_output_with_pretty(
        results: &[VerificationResult],
        format: OutputFormat,
        pretty: bool,
    ) -> String {
        match format {
            OutputFormat::Text => format_text_output(results),
            OutputFormat::Csv => format_csv_output(results),
            OutputFormat::Json => format_json_output_with_pretty(results, pretty),
        }
    }

    fn format_text_output(results: &[VerificationResult]) -> String {
        let mut output = String::from(
            "file_path\tstatus\texpected_hash\tcomputed_hash\tfile_size_bytes\tmodified_time\terror_message\n",
        );

        for result in results {
            let file_path = result.file().to_string_lossy();
            let status = result.status().display_string();
            let expected_hash = result.expected_hash();
            let computed_hash = if result.actual_hash().is_empty() {
                ""
            } else {
                result.actual_hash()
            };
            let file_size = result
                .file_size()
                .map_or_else(String::new, |s| s.to_string());
            let modified_time = result
                .modified_time()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(String::new, |d| d.as_secs().to_string());
            let error_message = result.error_message().unwrap_or("");

            writeln!(
                output,
                "{file_path}\t{status}\t{expected_hash}\t{computed_hash}\t{file_size}\t{modified_time}\t{error_message}"
            )
            .expect("Writing to String should never fail");
        }

        output.trim_end().to_string()
    }

    fn format_csv_output(results: &[VerificationResult]) -> String {
        let mut output = String::from(
            "file_path,status,expected_hash,computed_hash,file_size_bytes,modified_time,error_message\n",
        );

        for result in results {
            let file_path = result.file().to_string_lossy();
            let status = result.status().display_string();
            let expected_hash = result.expected_hash();
            let computed_hash = if result.actual_hash().is_empty() {
                ""
            } else {
                result.actual_hash()
            };
            let file_size = result
                .file_size()
                .map_or_else(String::new, |s| s.to_string());
            let modified_time = result
                .modified_time()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(String::new, |d| d.as_secs().to_string());
            let error_message = result.error_message().unwrap_or("");

            // Escape CSV fields
            let escaped_file_path = if file_path.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", file_path.replace('"', "\"\""))
            } else {
                file_path.to_string()
            };

            let escaped_error = if error_message.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", error_message.replace('"', "\"\""))
            } else {
                error_message.to_string()
            };

            writeln!(
                output,
                "{escaped_file_path},{status},{expected_hash},{computed_hash},{file_size},{modified_time},{escaped_error}"
            )
            .expect("Writing to String should never fail");
        }

        output.trim_end().to_string()
    }

    fn format_json_output_with_pretty(results: &[VerificationResult], pretty: bool) -> String {
        if pretty {
            format_json_output_pretty(results)
        } else {
            format_json_output_compact(results)
        }
    }

    fn format_json_output_compact(results: &[VerificationResult]) -> String {
        let mut json_objects = Vec::new();

        for result in results {
            let file_path = result.file().to_string_lossy();
            let status = result.status().display_string();
            let expected_hash = result.expected_hash();
            let computed_hash = if result.actual_hash().is_empty() {
                "null".to_string()
            } else {
                format!("\"{}\"", result.actual_hash())
            };
            let file_size = result
                .file_size()
                .map_or_else(|| "null".to_string(), |s| s.to_string());
            let modified_time = result
                .modified_time()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(|| "null".to_string(), |d| d.as_secs().to_string());
            let error_message = result.error_message().map_or_else(
                || "null".to_string(),
                |e| format!("\"{}\"", e.replace('\\', "\\\\").replace('"', "\\\"")),
            );

            // Escape JSON strings
            let escaped_file_path = escape_json_string(&file_path);

            json_objects.push(format!(
                "{{\"file_path\":\"{escaped_file_path}\",\"status\":\"{status}\",\"expected_hash\":\"{expected_hash}\",\"computed_hash\":{computed_hash},\"file_size_bytes\":{file_size},\"modified_time\":{modified_time},\"error_message\":{error_message}}}"
            ));
        }

        format!("[{}]", json_objects.join(","))
    }

    fn format_json_output_pretty(results: &[VerificationResult]) -> String {
        let mut output = String::from("[\n");
        for (i, result) in results.iter().enumerate() {
            let file_path = result.file().to_string_lossy();
            let status = result.status().display_string();
            let expected_hash = result.expected_hash();
            let computed_hash = if result.actual_hash().is_empty() {
                "null".to_string()
            } else {
                format!("\"{}\"", result.actual_hash())
            };
            let file_size = result
                .file_size()
                .map_or_else(|| "null".to_string(), |s| s.to_string());
            let modified_time = result
                .modified_time()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or_else(|| "null".to_string(), |d| d.as_secs().to_string());
            let error_message = result.error_message().map_or_else(
                || "null".to_string(),
                |e| format!("\"{}\"", e.replace('\\', "\\\\").replace('"', "\\\"")),
            );

            // Escape JSON strings
            let escaped_file_path = escape_json_string(&file_path);

            output.push_str("  {\n");
            writeln!(output, "    \"file_path\": \"{escaped_file_path}\",").unwrap();
            writeln!(output, "    \"status\": \"{status}\",").unwrap();
            writeln!(output, "    \"expected_hash\": \"{expected_hash}\",").unwrap();
            writeln!(output, "    \"computed_hash\": {computed_hash},").unwrap();
            writeln!(output, "    \"file_size_bytes\": {file_size},").unwrap();
            writeln!(output, "    \"modified_time\": {modified_time},").unwrap();
            writeln!(output, "    \"error_message\": {error_message}").unwrap();

            if i == results.len() - 1 {
                output.push_str("  }\n");
            } else {
                output.push_str("  },\n");
            }
        }
        output.push_str("]\n");
        output
    }

    // Helper function to escape JSON strings consistently
    fn escape_json_string(input: &str) -> String {
        input
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }
}

#[cfg(test)]
mod tests {
    use super::utils::*;
    use checkle::cli::OutputFormat;
    use checkle::prettyprint::VerificationResult;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn test_verification_report_with_missing_files() {
        // Test verification report includes missing files in output
        let missing_file = PathBuf::from("nonexistent.txt");
        let expected_hash = "abcdef1234567890".to_string();

        let result = VerificationResult::new_missing(missing_file.clone(), expected_hash.clone());
        let results = vec![result];

        let text_output =
            format_verification_output_with_pretty(&results, OutputFormat::Text, false);

        // Verify header is present
        assert!(text_output.contains("file_path\tstatus\texpected_hash"));
        // Verify missing file is included with MISSING status
        assert!(text_output.contains("nonexistent.txt"));
        assert!(text_output.contains("MISSING"));
        assert!(text_output.contains(&expected_hash));
    }

    #[test]
    fn test_verification_report_with_failed_files() {
        // Test verification report correctly handles failed verifications
        let file_path = PathBuf::from("corrupted.txt");
        let expected_hash = "abcdef1234567890".to_string();
        let computed_hash = "1234567890abcdef".to_string();
        let passed = false;

        let result = VerificationResult::new(
            file_path.clone(),
            expected_hash.clone(),
            computed_hash.clone(),
            passed,
        );
        let results = vec![result];

        let text_output =
            format_verification_output_with_pretty(&results, OutputFormat::Text, false);

        // Verify failed verification is properly reported
        assert!(text_output.contains("corrupted.txt"));
        assert!(text_output.contains("FAIL"));
        assert!(text_output.contains(&expected_hash));
        assert!(text_output.contains(&computed_hash));
    }

    #[test]
    fn test_verification_report_output_formats() {
        // Test all three output formats produce correct structure
        let file_path = PathBuf::from("test.txt");
        let expected_hash = "abcdef1234567890".to_string();
        let computed_hash = "abcdef1234567890".to_string();
        let passed = true;

        let result = VerificationResult::new(
            file_path.clone(),
            expected_hash.clone(),
            computed_hash.clone(),
            passed,
        );
        let results = vec![result];

        // Test Text format
        let text_output =
            format_verification_output_with_pretty(&results, OutputFormat::Text, false);
        assert!(text_output.contains("file_path\tstatus\texpected_hash")); // Tab-delimited header
        assert!(text_output.contains("test.txt\tPASS\t")); // Tab-delimited data

        // Test CSV format
        let csv_output = format_verification_output_with_pretty(&results, OutputFormat::Csv, false);
        assert!(csv_output.contains("file_path,status,expected_hash")); // CSV header
        assert!(csv_output.contains("test.txt,PASS,")); // CSV data

        // Test JSON format
        let json_output =
            format_verification_output_with_pretty(&results, OutputFormat::Json, false);
        assert!(json_output.starts_with('[') && json_output.ends_with(']')); // JSON array
        assert!(json_output.contains("\"file_path\":\"test.txt\"")); // JSON structure
        assert!(json_output.contains("\"status\":\"PASS\"")); // JSON data
        assert!(json_output.contains("\"expected_hash\":\"abcdef1234567890\"")); // Hash data
    }

    #[test]
    fn test_json_pretty_printing() {
        use crate::cli::OutputFormat;

        // Create test verification results
        let file_path = PathBuf::from("test.txt");
        let expected_hash = "abcdef1234567890".to_string();
        let computed_hash = "abcdef1234567890".to_string();
        let result = VerificationResult::new(file_path, expected_hash, computed_hash, true);
        let results = vec![result];

        // Test compact JSON (default)
        let compact_json =
            format_verification_output_with_pretty(&results, OutputFormat::Json, false);
        assert!(compact_json.starts_with('[') && compact_json.ends_with(']'));
        assert!(!compact_json.contains('\n')); // Should be single line
        assert!(compact_json.contains("\"file_path\":\"test.txt\""));

        // Test pretty JSON
        let pretty_json =
            format_verification_output_with_pretty(&results, OutputFormat::Json, true);
        assert!(pretty_json.starts_with("[\n"));
        assert!(pretty_json.ends_with("]\n"));
        assert!(pretty_json.contains("  {\n")); // Should have indentation
        assert!(pretty_json.contains("    \"file_path\": \"test.txt\"")); // Should have proper formatting
        assert!(pretty_json.contains("    \"status\": \"PASS\""));

        // Verify content is the same, just formatted differently
        // Check that both contain the same data, just with different formatting
        assert!(compact_json.contains("\"status\":\"PASS\""));
        assert!(pretty_json.contains("\"status\": \"PASS\""));
        assert!(compact_json.contains("\"file_path\":\"test.txt\""));
        assert!(pretty_json.contains("\"file_path\": \"test.txt\""));
    }

    #[test]
    fn test_format_auto_detection_from_file_extension() {
        use crate::cli::OutputFormat;

        // Test JSON detection
        let json_path = std::path::Path::new("report.json");
        assert_eq!(
            OutputFormat::detect_from_path(json_path),
            OutputFormat::Json
        );

        // Test CSV detection
        let csv_path = std::path::Path::new("report.csv");
        assert_eq!(OutputFormat::detect_from_path(csv_path), OutputFormat::Csv);

        // Test text fallback for unknown extension
        let unknown_path = std::path::Path::new("report.unknown");
        assert_eq!(
            OutputFormat::detect_from_path(unknown_path),
            OutputFormat::Text
        );

        // Test text fallback for no extension
        let no_ext_path = std::path::Path::new("report");
        assert_eq!(
            OutputFormat::detect_from_path(no_ext_path),
            OutputFormat::Text
        );

        // Test case insensitive detection
        let json_upper_path = std::path::Path::new("report.JSON");
        assert_eq!(
            OutputFormat::detect_from_path(json_upper_path),
            OutputFormat::Json
        );

        let csv_upper_path = std::path::Path::new("report.CSV");
        assert_eq!(
            OutputFormat::detect_from_path(csv_upper_path),
            OutputFormat::Csv
        );
    }

    #[test]
    fn test_parse_checksum_file_raw_includes_missing_files() {
        // Test that parse_checksum_file_raw includes missing files (unlike FilesToCheck)
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "abcdef1234567890\tmissing_file.txt").expect("Failed to write");
        writeln!(temp_file, "1234567890abcdef\tanother_missing.txt").expect("Failed to write");

        let pairs =
            parse_checksum_file_raw(temp_file.path()).expect("Failed to parse checksum file");

        // Verify both missing files are included
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].0, PathBuf::from("missing_file.txt"));
        assert_eq!(pairs[0].1, "abcdef1234567890");
        assert_eq!(pairs[1].0, PathBuf::from("another_missing.txt"));
        assert_eq!(pairs[1].1, "1234567890abcdef");
    }

    #[test]
    fn test_format_verification_output_comprehensive() {
        // Test comprehensive verification report with multiple file states
        let passed_file = VerificationResult::new(
            PathBuf::from("good.txt"),
            "abcdef1234567890".to_string(),
            "abcdef1234567890".to_string(),
            true,
        );

        let failed_file = VerificationResult::new(
            PathBuf::from("bad.txt"),
            "abcdef1234567890".to_string(),
            "1234567890abcdef".to_string(),
            false,
        );

        let missing_file = VerificationResult::new_missing(
            PathBuf::from("missing.txt"),
            "abcdef1234567890".to_string(),
        );

        let error_file = VerificationResult::new_error(
            PathBuf::from("error.txt"),
            "abcdef1234567890".to_string(),
            "Permission denied".to_string(),
        );

        let results = vec![passed_file, failed_file, missing_file, error_file];

        // Test that all verification states are properly represented in text output
        let text_output =
            format_verification_output_with_pretty(&results, OutputFormat::Text, false);
        assert!(text_output.contains("good.txt\tPASS"));
        assert!(text_output.contains("bad.txt\tFAIL"));
        assert!(text_output.contains("missing.txt\tMISSING"));
        assert!(text_output.contains("error.txt\tERROR: Permission denied"));

        // Test that JSON output correctly handles all states
        let json_output =
            format_verification_output_with_pretty(&results, OutputFormat::Json, false);
        assert!(json_output.contains("\"status\":\"PASS\""));
        assert!(json_output.contains("\"status\":\"FAIL\""));
        assert!(json_output.contains("\"status\":\"MISSING\""));
        assert!(json_output.contains("\"status\":\"ERROR: Permission denied\""));
    }
}
