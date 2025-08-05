use checkle::{
    cli::{self, Cli, Commands},
    commands,
};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use color_eyre::Result;
use std::io::stdout;

#[cfg(not(target_os = "windows"))]
fn main() -> Result<()> {
    run_main()
}

#[cfg(target_os = "windows")]
fn main() -> Result<()> {
    // On Windows, ensure we have adequate stack size (8MB) to prevent stack overflow
    // This is especially important for recursive operations and the preflight checks
    const STACK_SIZE: usize = 8 * 1024 * 1024; // 8MB

    // Build and run the main thread with explicit stack size
    let builder = std::thread::Builder::new()
        .name("main".into())
        .stack_size(STACK_SIZE);

    let handle = builder.spawn(run_main).unwrap();
    handle.join().unwrap()
}

fn run_main() -> Result<()> {
    // Parse provided command line arguments
    let cli = Cli::parse();

    // Set up logging, run preflight checks, and initialize thread pool
    preflight::setup(&cli.verbose, cli.threads)?;
    preflight::checks();

    // Get the hashing algorithm
    let algo = &cli.algorithm;

    // FOOD-FOR-THOUGHT: Consider implementing a method on the Command enum that will "dispatch"
    // to the appropriate command execution function, or even auto-populate a runtime state
    // structure that has an execute method (Execute trait?)
    match cli.command {
        // If no subcommand is provided, just print the tool's info
        #[allow(clippy::print_stderr)]
        None => {
            eprintln!("{}", cli::INFO);
            Ok(())
        }

        // Hash command
        Some(Commands::Hash {
            input_file,
            recursive,
            hash_output,
            format,
            pretty,
            per_file,
            no_progress,
            include,
            exclude,
            no_ignore,
            absolute_paths,
        }) => {
            commands::hash::execute(
                &input_file,
                recursive,
                hash_output.as_deref(),
                format,
                pretty,
                per_file,
                no_progress,
                &include,
                &exclude,
                no_ignore,
                *algo,
                cli.chunk_size_kb,
                cli.parallel_readers,
                cli.max_files_batch,
                absolute_paths,
            )?;
            Ok(())
        }

        // Verify single file
        Some(Commands::Verify {
            input_file,
            hash,
            per_file,
            pretty,
            no_progress,
            absolute_paths: _, // Not used in verify command - it only outputs OK/FAILED
        }) => {
            commands::verify::execute(
                &input_file,
                hash.as_deref(),
                *algo,
                per_file,
                pretty,
                no_progress,
                cli.chunk_size_kb,
                cli.parallel_readers,
            )?;
            Ok(())
        }

        // Verify many files
        Some(Commands::VerifyMany {
            checksum_file,
            per_file,
            files,
            pretty,
            report,
            format,
            max_files_batch,
            no_progress,
            absolute_paths,
        }) => {
            commands::verify_many::execute(
                checksum_file.as_deref(),
                per_file,
                &files,
                pretty,
                report.as_deref(),
                format,
                *algo,
                cli.chunk_size_kb,
                cli.parallel_readers,
                max_files_batch.unwrap_or(cli.max_files_batch),
                no_progress,
                absolute_paths,
            )?;
            Ok(())
        }

        // Generate shell completions
        Some(Commands::Completions { shell }) => {
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_string();

            generate(shell, &mut cmd, bin_name, &mut stdout());
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
    use std::{env, fs::File, io::Write, num::NonZeroUsize, sync::Once, thread};

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
        // Set up the logging verbosity as provided by the user
        let level = verbosity.log_level_filter();

        // Configure backtrace based on verbosity BEFORE installing color_eyre
        // Only show backtraces for debug (-vvv) and trace (-vvvv) levels
        let should_show_backtrace =
            matches!(level, log::LevelFilter::Debug | log::LevelFilter::Trace);

        // Check if user has set RUST_BACKTRACE
        let user_set_rust_backtrace = std::env::var("RUST_BACKTRACE").is_ok();
        let user_set_rust_lib_backtrace = std::env::var("RUST_LIB_BACKTRACE").is_ok();

        // If user hasn't set RUST_BACKTRACE or RUST_LIB_BACKTRACE, configure based on verbosity
        if !user_set_rust_backtrace && !user_set_rust_lib_backtrace {
            #[allow(clippy::disallowed_methods)] // Setting env var for error handling is required
            unsafe {
                // Set RUST_LIB_BACKTRACE which color_eyre respects
                // This must be done BEFORE color_eyre::install()
                std::env::set_var(
                    "RUST_LIB_BACKTRACE",
                    if should_show_backtrace { "1" } else { "0" },
                );
            }
        }

        // Set up color eyre AFTER setting environment variables
        color_eyre::install()?;

        // Set colors for the logs based on their level
        let colors = ColoredLevelConfig::new()
            .trace(Color::BrightBlue)
            .debug(Color::Blue)
            .warn(Color::Yellow)
            .error(Color::Red)
            .info(Color::Green);

        // Build and apply a new logger instance using fern and the user's desired verbosity
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
        for device in ["sda", "nvme0n1", "vda", "xvda"] {
            let rotational_path = format!("/sys/block/{device}/queue/rotational");
            if let Ok(content) = std::fs::read_to_string(&rotational_path)
                && content.trim() == "0"
            {
                return true;
            }
        }

        false
    }

    #[cfg(target_os = "macos")]
    fn check_macos_storage_type() -> bool {
        let output = std::process::Command::new("diskutil")
            .args(["info", "/"])
            .output()
            .ok();

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
        // Use a simpler PowerShell command to reduce complexity and potential stack usage
        let output = std::process::Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-PhysicalDisk | Where MediaType -eq 'SSD').Count -gt 0",
            ])
            .output()
            .ok();

        if let Some(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);
            matches!(output_str.trim(), "True" | "true")
        } else {
            // If PowerShell fails, assume not SSD to avoid crashes
            false
        }
    }

    fn check_temp_directory() {
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
