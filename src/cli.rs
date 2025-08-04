use std::{path::PathBuf, str::FromStr};

use crate::hashing::HashingAlgo;
use clap::{
    ArgAction, Error, Id, Parser, Subcommand,
    builder::{
        Styles, TypedValueParser, ValueParserFactory,
        styling::{AnsiColor, Effects},
    },
    error::ErrorKind,
};
use clap_complete::Shell;
use clap_verbosity_flag::WarnLevel;

pub const INFO: &str = r#"
   ___    _  _     ___     ___    _  __    _       ___   
  / __|  | || |   | __|   / __|  | |/ /   | |     | __|  
 | (__   | __ |   | _|   | (__   | ' <    | |__   | _|   
  \___|  |_||_|   |___|   \___|  |_|\_\   |____|  |___|  
_|"""""|_|"""""|_|"""""|_|"""""|_|"""""|_|"""""|_|"""""| 
"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-' 

checkle (v0.2.0)
------------------------------------------------------------
A `checksum` utility for the multicore age. `checkle` implements a Merkle hash
digest tree with parallelized nodes to progressively hash large files in small
chunks. It can be used to hash single files, hash lists of files, and generate
new hashes for files to be checked later.

New in v0.2.0: 
- Configurable parallel I/O for optimal performance on large files.
  Use --parallel-readers to control thread count and --chunk-size-kb to tune memory usage.
- Archive support: Read and verify files directly from TAR and ZIP archives
  without extraction using syntax like 'archive.tar:checksums.md5'.
"#;
pub const VERSION: &str = "v0.2.0";

/// Comprehensive examples for each checkle subcommand
pub const COMMAND_EXAMPLES: &str = r"EXAMPLES:
    # Hash a single file (creates checksum.txt and prints to stdout)
    checkle hash genome.fasta
    
    # Hash a file with SHA-256 algorithm
    checkle hash genome.fasta --algorithm sha256
    
    # Hash all files in current directory recursively
    checkle hash . -r
    
    # Hash files and write output to a specific file (no stdout)
    checkle hash *.fastq -o my_checksums.txt
    
    # Output in CSV format (auto-detected from extension)
    checkle hash *.fastq -o checksums.csv
    
    # Output in JSON format with explicit format flag
    checkle hash *.fastq -o output.txt --format json
    
    # Hash large dataset (progress bars show automatically unless silenced)
    checkle hash large_dataset/ -r
    
    # Hash files with a formatted table output to stderr for better readability
    checkle hash *.fastq --pretty
    
    # Verify a single file against a known hash
    checkle verify genome.fasta --hash 65a8e27d8879283831b664bd8b7f0ad4
    
    # Verify many files from a checksum list
    checkle verify-many -c checksums.txt
    
    # Use custom chunk size for performance tuning (in KB)
    checkle hash large_file.bam --chunk-size 2048
    
    # Control parallel readers for I/O optimization
    checkle hash *.fastq --parallel-readers 8
    
    # Increase verbosity to see detailed progress, or decrease to hide progress bars
    checkle hash data/ -r -vv           # Extra verbose with progress
    checkle hash data/ -r --quiet       # Silent mode, no progress bars";

/// Example usage for the verify command
pub const VERIFY_EXAMPLES: &str = r"Verify a file against a known hash value.

This command computes the hash of the specified file and compares it with the provided hash.
If they match, the file has not been corrupted or modified since the hash was generated.

EXAMPLES:
    # Verify with MD5 hash (default)
    checkle verify genome.fasta --hash 65a8e27d8879283831b664bd8b7f0ad4
    
    # Verify with SHA-256 hash
    checkle verify genome.fasta --hash d2d2d2... --algorithm sha256
    
    # Verify with custom performance settings
    checkle verify large_file.bam --hash abc123... --chunk-size 4096
    
    # Display verification results in a formatted table
    checkle verify genome.fasta --hash abc123... --pretty";

/// Example usage for the verify-many command
pub const VERIFY_MANY_EXAMPLES: &str = r"Verify multiple files using a checksum file.

This command reads a tab-delimited checksum file where each line contains:
    <hash><TAB><filepath>

All files listed will be verified in parallel, with a summary report at the end.

ARCHIVE SUPPORT:
    Checksum files can be read directly from within archives using the syntax:
    archive.tar:checksums.md5
    
    When a checksum file is within an archive, checkle will:
    1. First try to find each file within the same archive
    2. Fall back to the filesystem if not found in the archive
    3. Report the location where each file was verified

EXAMPLES:
    # Basic usage
    checkle verify-many -c checksums.txt
    
    # With SHA-256 algorithm
    checkle verify-many -c sha256sums.txt --algorithm sha256
    
    # Verify using a checksum file within a TAR archive
    checkle verify-many -c genome_data.tar:checksums.md5
    
    # Verify using a checksum file within a ZIP archive
    checkle verify-many -c results.zip:validation/checksums.sha256
    
    # With increased verbosity
    checkle verify-many -c checksums.txt -v
    
    # Display verification results in a formatted table with summary
    checkle verify-many -c checksums.txt --pretty
    
CHECKSUM FILE FORMAT:
    65a8e27d8879283831b664bd8b7f0ad4	data/sample1.fastq
    1f4d99d2c6591a69793e27d4ffe87156	data/sample2.fastq
    7771ae6c75b6909ecce7228d6391bf46	results/output.bam";

/// Example usage for the hash command
pub const HASH_EXAMPLES: &str = r"Generate checksums for one or more files.

This command computes cryptographic hashes for files using either MD5 (default) or SHA-256.
The results are written to both 'checksum.txt' and stdout, unless --hash-output is specified.

EXAMPLES:
    # Hash a single file
    checkle hash genome.fasta
    
    # Hash all FASTQ files in current directory
    checkle hash *.fastq
    
    # Hash directory recursively with progress bars
    checkle hash data/ -r
    
    # Disable progress bars for scripting
    checkle hash data/ -r --no-progress
    
    # Write hashes to custom file (no stdout output)
    checkle hash *.bam -o bam_checksums.txt
    
    # Export results in CSV format for spreadsheet analysis
    checkle hash *.bam -o results.csv
    
    # Generate JSON output for programmatic processing
    checkle hash *.bam --format json -o api_output.json
    
    # Use SHA-256 for better security
    checkle hash sensitive_data/ -r --algorithm sha256
    
    # Optimize for large files with custom settings
    checkle hash huge_genome.fa --chunk-size 4096 --parallel-readers 16
    
    # Display results in a formatted table to stderr for better readability
    checkle hash *.fastq --pretty
    
    # Include only specific file patterns
    checkle hash . -r --include '*.rs' --include '*.toml'
    
    # Exclude temporary and build files
    checkle hash src/ -r --exclude '*.tmp' --exclude 'target/'
    
    # Include only FASTQ files, exclude temporary ones
    checkle hash data/ -r --include '*.fastq' --exclude '*.tmp.fastq'
    
    # Process all files including those in .gitignore
    checkle hash . -r --no-ignore
    
    # Complex filtering: Rust files only, no tests, ignore .gitignore
    checkle hash . -r --include '*.rs' --exclude '*.test.rs' --no-ignore
    
OUTPUT FORMAT:
    The output is tab-delimited with format: <hash><TAB><filepath>
    This format is compatible with the verify-many command.";

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Yellow.on_default());

#[derive(Debug, Parser)]
#[clap(name = "checkle")]
#[command(author, version = VERSION, about = INFO, long_about = None)]
#[command(after_help = COMMAND_EXAMPLES)]
#[command(propagate_version = true)]
#[command(styles = STYLES)]
pub struct Cli {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity<WarnLevel>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short = 'j', long, required = false, global = true)]
    pub threads: Option<usize>,

    #[arg(short, long, required = false, global = true, default_value = "md5", value_parser = HashingAlgo::value_parser())]
    pub algorithm: HashingAlgo,

    /// Size of chunks to process (in KB). Smaller chunks allow more
    /// parallelism but have higher overhead.
    #[arg(
        long,
        value_name = "SIZE_KB",
        default_value = "256",
        global = true,
        help_heading = "Performance Options",
        value_parser = utils::parse_chunk_size_kb
    )]
    pub chunk_size_kb: usize,

    /// Number of parallel readers for large files.
    /// Set to 1 to disable parallel I/O, 0 for auto-detect (default).
    #[arg(
        long,
        short = 'p',
        value_name = "COUNT",
        default_value = "0",
        global = true,
        help_heading = "Performance Options",
        value_parser = utils::parse_parallel_readers
    )]
    pub parallel_readers: usize,
    /// Maximum number of files to process in a single batch.
    /// Increase this if your system has sufficient memory and you need to process more files.
    #[arg(
        long,
        value_name = "COUNT",
        default_value = "10000",
        global = true,
        help_heading = "Performance Options",
        value_parser = utils::parse_max_files_batch
    )]
    pub max_files_batch: usize,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[clap(
        about = "Verify a file with the provided hash.",
        long_about = VERIFY_EXAMPLES,
        visible_aliases = &["check", "checksum", "c", "ver", "verfy", "verfiy", "v"]
    )]
    Verify {
        #[arg(index = 1, required = true)]
        input_file: PathBuf,

        #[arg(long, required_unless_present = "per_file")]
        hash: Option<String>,

        #[arg(
            long = "per-file",
            help = "Read hash from a file alongside the source file (e.g., file.txt.md5)",
            action = ArgAction::SetTrue,
            conflicts_with = "hash"
        )]
        per_file: PerFileMode,

        #[arg(
            short = 'P',
            long = "pretty",
            help = "Display verification results in a formatted table to stderr",
            help_heading = "Output Options",
            action = ArgAction::SetTrue
        )]
        pretty: PrettyPrint,
    },

    #[clap(
            about = "Perform checksums on all the files in a provided text file",
            long_about = VERIFY_MANY_EXAMPLES,
            visible_aliases = &["all", "many", "list", "a", "m", "queue", "q"]
        )]
    VerifyMany {
        #[arg(short, long, required_unless_present = "per_file")]
        checksum_file: Option<PathBuf>,

        #[arg(
            long = "per-file",
            help = "Read hashes from individual files alongside each source file (e.g., file.txt.md5)",
            action = ArgAction::SetTrue,
            conflicts_with = "checksum_file"
        )]
        per_file: PerFileMode,

        #[arg(index = 1, help = "Files to verify (required when using --per-file)")]
        files: Vec<PathBuf>,

        #[arg(
            short = 'P',
            long = "pretty",
            help = "Display verification results in a formatted table to stderr with summary",
            help_heading = "Output Options",
            action = ArgAction::SetTrue
        )]
        pretty: PrettyPrint,

        #[arg(
            short = 'o',
            long = "report",
            help = "Write verification report to file (format determined by extension or --format)",
            help_heading = "Output Options",
            value_name = "FILE"
        )]
        report: Option<PathBuf>,

        #[arg(
            short = 'F',
            long = "format",
            help = "Output format for report (text/csv/json)",
            help_heading = "Output Options",
            value_enum,
            conflicts_with = "pretty"
        )]
        format: Option<OutputFormat>,
    },

    #[clap(
            about = "Compute hashes for any input file(s) that can be checksummed later.",
            long_about = HASH_EXAMPLES,
            visible_aliases = &["h", "n", "init", "new", "g", "gen", "generate"]
        )]
    Hash {
        #[arg(index = 1, default_value = "./*")]
        input_file: PathBuf,

        #[arg(
            short = 'r',
            long,
            help = "Recursively hash files in directories",
            help_heading = "Directory Options",
            action = ArgAction::SetTrue
        )]
        recursive: Recursive,

        #[arg(
            short = 'o',
            long = "hash-output",
            help = "Write hashes to specified file instead of stdout",
            help_heading = "Output Options",
            value_name = "FILE",
            conflicts_with = "per_file"
        )]
        hash_output: Option<PathBuf>,

        #[arg(
            short = 'f',
            long = "format",
            help = "Output format (text, csv, json). Auto-detected from output file extension if not specified",
            help_heading = "Output Options",
            value_name = "FORMAT",
            value_parser = OutputFormat::value_parser(),
            conflicts_with = "per_file"
        )]
        format: Option<OutputFormat>,

        #[arg(
            short = 'P',
            long = "pretty",
            help = "Display results in a formatted table to stderr",
            help_heading = "Output Options",
            action = ArgAction::SetTrue
        )]
        pretty: PrettyPrint,

        #[arg(
            long = "per-file",
            help = "Write each hash to a separate file alongside the source file (e.g., file.txt.md5)",
            help_heading = "Output Options",
            action = ArgAction::SetTrue,
            conflicts_with = "hash_output",
            conflicts_with = "format"
        )]
        per_file: PerFileMode,

        /// Hide progress bars during operation.
        #[arg(
            long,
            default_value = "false",
            help_heading = "Display Options",
            action = ArgAction::SetTrue
        )]
        no_progress: bool,

        /// Include only files matching this glob pattern (can be specified multiple times).
        /// Patterns follow gitignore syntax. Example: --include "*.rs" --include "src/*.txt"
        #[arg(
            short = 'i',
            long = "include",
            value_name = "PATTERN",
            help_heading = "Filter Options",
            action = ArgAction::Append
        )]
        include: Vec<String>,

        /// Exclude files matching this glob pattern (can be specified multiple times).
        /// Patterns follow gitignore syntax. Example: --exclude "*.tmp" --exclude "target/"
        #[arg(
            short = 'e',
            long = "exclude",
            value_name = "PATTERN",
            help_heading = "Filter Options",
            action = ArgAction::Append
        )]
        exclude: Vec<String>,

        /// Don't respect .gitignore files when traversing directories.
        /// By default, files listed in .gitignore are excluded from hashing.
        #[arg(
            long = "no-ignore",
            help_heading = "Filter Options",
            action = ArgAction::SetTrue
        )]
        no_ignore: bool,
    },

    #[clap(
        about = "Generate shell completion scripts",
        long_about = "Generate shell completion scripts for various shells.\n\n\
                     To use the generated completions:\n\n\
                     Bash:\n  \
                       checkle completions bash > ~/.local/share/bash-completion/completions/checkle\n\n\
                     Zsh:\n  \
                       checkle completions zsh > ~/.zfunc/_checkle\n  \
                       # Add this to your ~/.zshrc: fpath=(~/.zfunc $fpath)\n\n\
                     Fish:\n  \
                       checkle completions fish > ~/.config/fish/completions/checkle.fish\n\n\
                     PowerShell:\n  \
                       checkle completions powershell >> $PROFILE\n\n\
                     Elvish:\n  \
                       checkle completions elvish > ~/.config/elvish/lib/checkle.elv\n  \
                       # Add this to ~/.config/elvish/rc.elv: use checkle",
        visible_aliases = &["completion", "comp"]
    )]
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

// Implement FromStr for HashingAlgo to enable string parsing
impl FromStr for HashingAlgo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "md5" => Ok(HashingAlgo::Md5),
            "sha2" | "sha-2" | "sha256" | "sha-256" => Ok(HashingAlgo::Sha2),
            _ => Err(format!(
                "Unknown hashing algorithm: {s}. Valid options are 'md5' or 'sha2'"
            )),
        }
    }
}

// Define our custom parser that implements TypedValueParser
#[derive(Clone, Debug)]
pub struct HashingAlgoParser;

impl TypedValueParser for HashingAlgoParser {
    type Value = HashingAlgo;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, Error> {
        // Convert OsStr to a regular &str
        let value_str = value.to_str().ok_or_else(|| {
            Error::raw(
                ErrorKind::InvalidUtf8,
                "Hashing algorithm must be valid UTF-8",
            )
        })?;

        // Use our FromStr implementation to parse the string
        HashingAlgo::from_str(value_str).map_err(|err| {
            let default_id = Id::from("md5");
            let arg_name = arg.map_or(&default_id, |a| a.get_id());
            Error::raw(
                ErrorKind::InvalidValue,
                format!("Invalid value for {arg_name}: {err}"),
            )
        })
    }
}

// Implement ValueParserFactory to allow using HashingAlgo::value_parser()
impl ValueParserFactory for HashingAlgo {
    type Parser = HashingAlgoParser;

    fn value_parser() -> Self::Parser {
        HashingAlgoParser
    }
}

// Implement FromStr for OutputFormat to enable string parsing
impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" | "txt" | "tab" => Ok(OutputFormat::Text),
            "csv" => Ok(OutputFormat::Csv),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!(
                "Unknown output format: {s}. Valid options are 'text', 'csv', or 'json'"
            )),
        }
    }
}

// Define our custom parser that implements TypedValueParser
#[derive(Clone, Debug)]
pub struct OutputFormatParser;

impl TypedValueParser for OutputFormatParser {
    type Value = OutputFormat;

    fn parse_ref(
        &self,
        _cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, Error> {
        // Convert OsStr to a regular &str
        let value_str = value.to_str().ok_or_else(|| {
            Error::raw(ErrorKind::InvalidUtf8, "Output format must be valid UTF-8")
        })?;

        // Use our FromStr implementation to parse the string
        OutputFormat::from_str(value_str).map_err(|err| {
            let default_id = Id::from("text");
            let arg_name = arg.map_or(&default_id, |a| a.get_id());
            Error::raw(
                ErrorKind::InvalidValue,
                format!("Invalid value for {arg_name}: {err}"),
            )
        })
    }
}

// Implement ValueParserFactory to allow using OutputFormat::value_parser()
impl ValueParserFactory for OutputFormat {
    type Parser = OutputFormatParser;

    fn value_parser() -> Self::Parser {
        OutputFormatParser
    }
}

/// Output format for hash results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Tab-delimited text format (default): hash\tfilepath
    Text,
    /// CSV format with proper escaping: hash,filepath
    Csv,
    /// JSON format with structured data
    Json,
}

impl OutputFormat {
    /// Detect output format from file extension
    ///
    /// # Arguments
    /// * `path` - The output file path to analyze
    ///
    /// # Returns
    /// The detected format based on file extension, or Text if no specific format detected
    ///
    /// # Panics
    /// This function will not panic as it handles all edge cases gracefully.
    #[must_use]
    pub fn detect_from_path(path: &std::path::Path) -> Self {
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                match ext_str.to_lowercase().as_str() {
                    "json" => OutputFormat::Json,
                    "csv" => OutputFormat::Csv,
                    _ => OutputFormat::Text,
                }
            } else {
                OutputFormat::Text
            }
        } else {
            OutputFormat::Text
        }
    }

    /// Get the file extension typically associated with this format
    ///
    /// # Returns
    /// The typical file extension for this format (without the dot)
    #[must_use]
    pub const fn typical_extension(self) -> &'static str {
        match self {
            OutputFormat::Text => "txt",
            OutputFormat::Csv => "csv",
            OutputFormat::Json => "json",
        }
    }
}

// Type aliases for boolean flags - self-documenting per project conventions
pub type Recursive = bool;

/// Type alias for per-file hash storage mode flag.
///
/// Controls whether hash values are stored in individual files alongside
/// each source file (e.g., file.txt.md5) rather than in a consolidated
/// checksum file. This mode is compatible with traditional checksum tools
/// and useful for workflows where hash files travel with their source files.
///
/// When enabled:
/// - For hashing: Creates individual .md5 or .sha256 files next to each input file
/// - For verification: Reads hash values from individual files instead of --hash parameter
///
/// # Examples
///
/// ```rust
/// use checkle::cli::PerFileMode;
///
/// let per_file_mode: PerFileMode = true;  // Enable per-file hash storage
/// let standard_mode: PerFileMode = false; // Use consolidated checksum file
/// ```
pub type PerFileMode = bool;

/// Type alias for pretty printing mode flag.
///
/// Controls whether output is displayed in a formatted table to stderr
/// for improved readability. When enabled, checkle will display hash
/// results in a visually formatted table alongside the normal output.
/// This provides better semantic meaning than a raw bool and allows
/// for future extensibility of the pretty printing feature.
///
/// # Examples
///
/// ```rust
/// use checkle::cli::PrettyPrint;
///
/// let pretty_mode: PrettyPrint = true;  // Enable formatted table output
/// let normal_mode: PrettyPrint = false; // Disable formatted output
/// ```
pub type PrettyPrint = bool;

mod utils {
    use crate::errors::CheckleError;

    // Validation functions for CLI parameters
    pub(super) fn parse_chunk_size_kb(s: &str) -> Result<usize, String> {
        let kb: usize = s.parse().map_err(|_| {
            CheckleError::InvalidNumericValue {
                value: s.to_string(),
                reason: "not a valid number".to_string(),
            }
            .to_string()
        })?;

        if kb < 4 {
            return Err(CheckleError::InvalidCliArgument(
                "Chunk size must be at least 4 KB".to_string(),
            )
            .to_string());
        }
        if kb > 65536 {
            return Err(CheckleError::InvalidCliArgument(
                "Chunk size cannot exceed 64 MB (65536 KB)".to_string(),
            )
            .to_string());
        }

        Ok(kb)
    }

    pub(super) fn parse_parallel_readers(s: &str) -> Result<usize, String> {
        let readers: usize = s.parse().map_err(|_| {
            CheckleError::InvalidNumericValue {
                value: s.to_string(),
                reason: "not a valid number".to_string(),
            }
            .to_string()
        })?;

        if readers > 64 {
            return Err(CheckleError::InvalidCliArgument(
                "Parallel readers cannot exceed 64".to_string(),
            )
            .to_string());
        }

        Ok(readers)
    }

    pub(super) fn parse_max_files_batch(s: &str) -> Result<usize, String> {
        // Precondition assertions (Tiger Style)
        assert!(!s.is_empty(), "Input string must not be empty");
        assert!(s.len() <= 20, "Input string too long for numeric parsing");

        let max_files: usize = s.parse().map_err(|_| {
            CheckleError::InvalidNumericValue {
                value: s.to_string(),
                reason: "not a valid number".to_string(),
            }
            .to_string()
        })?;

        if max_files < crate::constants::MIN_FILES_BATCH_LIMIT {
            return Err(CheckleError::InvalidCliArgument(
                format!("Maximum files batch size must be at least {}. Recommended values: 1000-50000 for typical systems, up to {} for high-memory servers", 
                    crate::constants::MIN_FILES_BATCH_LIMIT,
                    crate::constants::MAX_FILES_BATCH_LIMIT),
            )
            .to_string());
        }
        if max_files > crate::constants::MAX_FILES_BATCH_LIMIT {
            return Err(CheckleError::InvalidCliArgument(
                format!("Maximum files batch size cannot exceed {}. For extremely large directories, consider using more specific filters (--include, --exclude)", 
                    crate::constants::MAX_FILES_BATCH_LIMIT),
            )
            .to_string());
        }

        // Postcondition assertions (Tiger Style)
        assert!(
            max_files >= crate::constants::MIN_FILES_BATCH_LIMIT,
            "Parsed value must meet minimum"
        );
        assert!(
            max_files <= crate::constants::MAX_FILES_BATCH_LIMIT,
            "Parsed value must not exceed maximum"
        );

        Ok(max_files)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::uninlined_format_args,
        clippy::expect_fun_call,
        clippy::const_is_empty
    )]
    use super::*;
    use clap::Parser;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, FileFailurePersistence};
    use std::ffi::OsStr;
    use utils::*;

    // Test 1: Normal operation - HashingAlgo FromStr parsing
    #[test]
    fn test_hashing_algo_from_str_normal() {
        // Test MD5 variants
        assert_eq!("md5".parse::<HashingAlgo>().unwrap(), HashingAlgo::Md5);
        assert_eq!("MD5".parse::<HashingAlgo>().unwrap(), HashingAlgo::Md5);
        assert_eq!("Md5".parse::<HashingAlgo>().unwrap(), HashingAlgo::Md5);

        // Test SHA2 variants
        assert_eq!("sha2".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("SHA2".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("sha-2".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("sha256".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("SHA256".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("sha-256".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
        assert_eq!("SHA-256".parse::<HashingAlgo>().unwrap(), HashingAlgo::Sha2);
    }

    // Test 2: Normal operation - CLI parsing with verify command
    #[test]
    fn test_cli_verify_command_parsing() {
        let args = vec![
            "checkle",
            "verify",
            "/path/to/file.txt",
            "--hash",
            "abcdef1234567890abcdef1234567890",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse verify command");

        match cli.command {
            Some(Commands::Verify {
                input_file,
                hash,
                per_file: _,
                pretty: _,
            }) => {
                assert_eq!(input_file, PathBuf::from("/path/to/file.txt"));
                assert_eq!(hash, Some("abcdef1234567890abcdef1234567890".to_string()));
            }
            _ => panic!("Expected Verify command"),
        }

        // Should default to MD5
        assert_eq!(cli.algorithm, HashingAlgo::Md5);
    }

    // Test 3: Normal operation - CLI parsing with hash command
    #[test]
    fn test_cli_hash_command_parsing() {
        let args = vec!["checkle", "hash", "/path/to/input.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse hash command");

        match cli.command {
            Some(Commands::Hash { input_file, .. }) => {
                assert_eq!(input_file, PathBuf::from("/path/to/input.txt"));
            }
            _ => panic!("Expected Hash command"),
        }
    }

    // Test 4: Normal operation - CLI parsing with verify-many command
    #[test]
    fn test_cli_verify_many_command_parsing() {
        let args = vec![
            "checkle",
            "verify-many",
            "--checksum-file",
            "/path/to/checksums.txt",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse verify-many command");

        match cli.command {
            Some(Commands::VerifyMany {
                checksum_file,
                per_file: _,
                files: _,
                pretty: _,
                report: _,
                format: _,
            }) => {
                assert_eq!(checksum_file, Some(PathBuf::from("/path/to/checksums.txt")));
            }
            _ => panic!("Expected VerifyMany command"),
        }
    }

    // Test 5: Normal operation - CLI parsing with algorithm specification
    #[test]
    fn test_cli_algorithm_specification() {
        let args = vec![
            "checkle",
            "--algorithm",
            "sha256",
            "hash",
            "/path/to/file.txt",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse with algorithm");
        assert_eq!(cli.algorithm, HashingAlgo::Sha2);
    }

    // Test 6: Normal operation - CLI parsing with threads specification
    #[test]
    fn test_cli_threads_specification() {
        let args = vec!["checkle", "--threads", "8", "hash", "/path/to/file.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse with threads");
        assert_eq!(cli.threads, Some(8));
    }

    // Test 7: Normal operation - CLI parsing with verbose flag
    #[test]
    fn test_cli_verbose_specification() {
        let args = vec!["checkle", "-v", "hash", "/path/to/file.txt"];

        let _cli = Cli::try_parse_from(args).expect("Should parse with verbose");
        // The verbose field is handled by clap_verbosity_flag, just ensure parsing succeeds
    }

    // Test 8: Edge case - CLI parsing with command aliases
    #[test]
    fn test_cli_command_aliases() {
        // Test verify aliases
        let verify_aliases = vec!["check", "checksum", "c", "ver", "verfy", "verfiy", "v"];
        for alias in verify_aliases {
            let args = vec!["checkle", alias, "/file.txt", "--hash", "abc123"];
            let cli = Cli::try_parse_from(args).expect(&format!("Should parse alias {}", alias));
            assert!(matches!(cli.command, Some(Commands::Verify { .. })));
        }

        // Test verify-many aliases
        let verify_many_aliases = vec!["all", "many", "list", "a", "m", "queue", "q"];
        for alias in verify_many_aliases {
            let args = vec!["checkle", alias, "--checksum-file", "/checksums.txt"];
            let cli = Cli::try_parse_from(args).expect(&format!("Should parse alias {}", alias));
            assert!(matches!(cli.command, Some(Commands::VerifyMany { .. })));
        }

        // Test hash aliases
        let hash_aliases = vec!["h", "n", "init", "new", "g", "gen", "generate"];
        for alias in hash_aliases {
            let args = vec!["checkle", alias, "/file.txt"];
            let cli = Cli::try_parse_from(args).expect(&format!("Should parse alias {}", alias));
            assert!(matches!(cli.command, Some(Commands::Hash { .. })));
        }
    }

    // Test 9: Edge case - default values
    #[test]
    fn test_cli_default_values() {
        let args = vec!["checkle", "hash"];

        let cli = Cli::try_parse_from(args).expect("Should parse with defaults");

        // Algorithm should default to MD5
        assert_eq!(cli.algorithm, HashingAlgo::Md5);

        // Threads should be None (not specified)
        assert_eq!(cli.threads, None);

        // Hash command should have default input
        match cli.command {
            Some(Commands::Hash { input_file, .. }) => {
                assert_eq!(input_file, PathBuf::from("./*"));
            }
            _ => panic!("Expected Hash command"),
        }
    }

    // Test 10: Error path - invalid algorithm
    #[test]
    fn test_hashing_algo_from_str_invalid() {
        let result = "invalid_algo".parse::<HashingAlgo>();
        assert!(result.is_err(), "Invalid algorithm should fail to parse");

        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Unknown hashing algorithm"));
        assert!(err_msg.contains("invalid_algo"));
    }

    // Test 11: Error path - CLI parsing failure (missing required argument)
    #[test]
    fn test_cli_parsing_missing_required_arg() {
        let args = vec!["checkle", "verify", "/file.txt"]; // Missing --hash

        let result = Cli::try_parse_from(args);
        assert!(
            result.is_err(),
            "Should fail when required argument is missing"
        );
    }

    // Test 12: Error path - CLI parsing failure (invalid threads value)
    #[test]
    fn test_cli_parsing_invalid_threads() {
        let args = vec!["checkle", "--threads", "not_a_number", "hash", "/file.txt"];

        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Should fail with invalid threads value");
    }

    // Test 13: HashingAlgoParser functionality
    #[test]
    fn test_hashing_algo_parser() {
        let parser = HashingAlgoParser;

        // Test valid parsing
        let result = parser.parse_ref(&clap::Command::new("test"), None, OsStr::new("md5"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), HashingAlgo::Md5);

        // Test invalid parsing
        let result = parser.parse_ref(&clap::Command::new("test"), None, OsStr::new("invalid"));
        assert!(result.is_err());
    }

    // Test 14: HashingAlgoParser with non-UTF8 input
    #[test]
    fn test_hashing_algo_parser_non_utf8() {
        let parser = HashingAlgoParser;

        // Create invalid UTF-8 OsStr (this is platform-specific behavior)
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid_utf8 = OsStr::from_bytes(&[0xFF, 0xFE]);

            let result = parser.parse_ref(&clap::Command::new("test"), None, invalid_utf8);
            assert!(result.is_err());
        }
    }

    // Test 15: CLI version and help info
    #[test]
    fn test_cli_version_constant() {
        assert_eq!(VERSION, "v0.2.0");
        assert!(!INFO.is_empty());
        assert!(INFO.contains("checkle"));
        assert!(INFO.contains("Merkle"));
        assert!(INFO.contains("parallel I/O"));
    }

    // Test 16: Edge case - CLI parsing with empty arguments
    #[test]
    fn test_cli_parsing_empty_args() {
        let args = vec!["checkle"]; // Just the program name

        let result = Cli::try_parse_from(args);
        assert!(result.is_ok(), "CLI should handle no command gracefully");

        let cli = result.unwrap();
        assert!(
            cli.command.is_none(),
            "Should have no command when none specified"
        );
        assert_eq!(cli.algorithm, HashingAlgo::Md5, "Should default to MD5");
    }

    // Test 17: Edge case - CLI parsing with excessive verbose flags
    #[test]
    fn test_cli_parsing_excessive_verbose() {
        let args = vec!["checkle", "-vvvvv", "hash", "/tmp/test.txt"];

        let result = Cli::try_parse_from(args);
        assert!(result.is_ok(), "Should handle excessive verbose flags");

        // The verbosity level is handled by clap_verbosity_flag
        let cli = result.unwrap();
        assert!(matches!(cli.command, Some(Commands::Hash { .. })));
    }

    // Test 18: Edge case - CLI parsing with conflicting options
    #[test]
    fn test_cli_parsing_algorithm_case_sensitivity() {
        let test_cases = vec![
            ("md5", HashingAlgo::Md5),
            ("MD5", HashingAlgo::Md5),
            ("Md5", HashingAlgo::Md5),
            ("sha2", HashingAlgo::Sha2),
            ("SHA2", HashingAlgo::Sha2),
            ("sha256", HashingAlgo::Sha2),
            ("SHA-256", HashingAlgo::Sha2),
        ];

        for (algo_str, expected) in test_cases {
            let args = vec!["checkle", "--algorithm", algo_str, "hash", "/tmp/test.txt"];
            let result = Cli::try_parse_from(args);
            assert!(result.is_ok(), "Should parse algorithm: {}", algo_str);

            let cli = result.unwrap();
            assert_eq!(
                cli.algorithm, expected,
                "Algorithm {} should parse to {:?}",
                algo_str, expected
            );
        }
    }

    // Test 19: Error path - CLI parsing with invalid threads values
    #[test]
    fn test_cli_parsing_threads_edge_cases() {
        // Test zero threads
        let args = vec!["checkle", "--threads", "0", "hash", "/tmp/test.txt"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_ok(),
            "Zero threads should be allowed (clap will accept it)"
        );

        // Test negative threads (should be rejected by clap type parsing)
        let args = vec!["checkle", "--threads", "-1", "hash", "/tmp/test.txt"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Negative threads should be rejected");

        // Test very large thread count
        let args = vec!["checkle", "--threads", "999999", "hash", "/tmp/test.txt"];
        let result = Cli::try_parse_from(args);
        assert!(
            result.is_ok(),
            "Large thread count should be parsed (validation happens elsewhere)"
        );
    }

    // Test 20: Error path - CLI parsing with malformed arguments
    #[test]
    fn test_cli_parsing_malformed_arguments() {
        // Missing argument value
        let args = vec!["checkle", "--algorithm"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Missing algorithm value should fail");

        // Missing required hash for verify
        let args = vec!["checkle", "verify", "/tmp/test.txt"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Missing hash for verify should fail");

        // Missing required checksum file for verify-many
        let args = vec!["checkle", "verify-many"];
        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Missing checksum file should fail");
    }

    // Test 21: Edge case - CLI with very long file paths
    #[test]
    fn test_cli_long_file_paths() {
        let long_path = "/very/long/path/".repeat(50) + "file.txt"; // Very long path
        let args = vec!["checkle", "hash", &long_path];

        let result = Cli::try_parse_from(args);
        assert!(result.is_ok(), "Long file paths should be accepted");

        let cli = result.unwrap();
        match cli.command {
            Some(Commands::Hash { input_file, .. }) => {
                assert_eq!(input_file, PathBuf::from(&long_path));
            }
            _ => panic!("Expected Hash command"),
        }
    }

    // Test 22: Edge case - HashingAlgoParser error conditions
    #[test]
    fn test_hashing_algo_parser_edge_cases() {
        let parser = HashingAlgoParser;

        // Test empty string
        let result = parser.parse_ref(&clap::Command::new("test"), None, OsStr::new(""));
        assert!(result.is_err(), "Empty string should fail to parse");

        // Test whitespace-only string
        let result = parser.parse_ref(&clap::Command::new("test"), None, OsStr::new("   "));
        assert!(
            result.is_err(),
            "Whitespace-only string should fail to parse"
        );

        // Test string with numbers
        let result = parser.parse_ref(&clap::Command::new("test"), None, OsStr::new("md51"));
        assert!(
            result.is_err(),
            "Algorithm with numbers should fail to parse"
        );
    }

    // Test 23: Edge case - CLI info string and version consistency
    #[test]
    fn test_cli_info_and_version_consistency() {
        // Test that INFO contains expected elements
        assert!(INFO.contains("checkle"), "INFO should contain program name");
        assert!(INFO.contains("v0.2.0"), "INFO should contain version");
        assert!(INFO.contains("Merkle"), "INFO should mention Merkle trees");
        assert!(
            INFO.contains("parallel I/O"),
            "INFO should mention parallel I/O"
        );
        assert!(
            INFO.contains("--parallel-readers"),
            "INFO should mention CLI flags"
        );

        // Test version string format
        assert!(VERSION.starts_with('v'), "Version should start with 'v'");
        assert_eq!(VERSION, "v0.2.0", "Version should match expected value");

        // Test that INFO and VERSION are consistent
        assert!(
            INFO.contains(VERSION),
            "INFO should contain the VERSION string"
        );
    }

    // Test 24: Stress test - CLI parsing with many aliases
    #[test]
    fn test_cli_aliases_stress_test() {
        // Test all verify aliases in a loop
        let verify_aliases = vec!["check", "checksum", "c", "ver", "verfy", "verfiy", "v"];
        for alias in &verify_aliases {
            for other_alias in &verify_aliases {
                if alias != other_alias {
                    let args = vec!["checkle", alias, "/file.txt", "--hash", "abc123"];
                    let result = Cli::try_parse_from(args);
                    assert!(result.is_ok(), "All verify aliases should work: {}", alias);
                }
            }
        }
    }

    // Test 25: New CLI fields - chunk size KB parsing
    #[test]
    fn test_cli_chunk_size_kb_parsing() {
        let args = vec!["checkle", "--chunk-size-kb", "512", "hash", "/tmp/test.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse chunk size");
        assert_eq!(
            cli.chunk_size_kb, 512,
            "Chunk size should be parsed correctly"
        );
        assert_eq!(
            cli.parallel_readers, 0,
            "Parallel readers should default to 0"
        );
    }

    // Test 26: New CLI fields - parallel readers parsing
    #[test]
    fn test_cli_parallel_readers_parsing() {
        let args = vec![
            "checkle",
            "--parallel-readers",
            "8",
            "hash",
            "/tmp/test.txt",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse parallel readers");
        assert_eq!(
            cli.parallel_readers, 8,
            "Parallel readers should be parsed correctly"
        );
        assert_eq!(cli.chunk_size_kb, 256, "Chunk size should default to 256");
    }

    // Test 27: New CLI fields - short form parallel readers parsing
    #[test]
    fn test_cli_parallel_readers_short_form() {
        let args = vec!["checkle", "-p", "4", "hash", "/tmp/test.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse parallel readers short form");
        assert_eq!(
            cli.parallel_readers, 4,
            "Parallel readers should be parsed correctly with -p"
        );
    }

    // Test 28: New CLI fields - both options together
    #[test]
    fn test_cli_both_performance_options() {
        let args = vec![
            "checkle",
            "--chunk-size-kb",
            "1024",
            "--parallel-readers",
            "16",
            "hash",
            "/tmp/test.txt",
        ];

        let cli = Cli::try_parse_from(args).expect("Should parse both performance options");
        assert_eq!(
            cli.chunk_size_kb, 1024,
            "Chunk size should be parsed correctly"
        );
        assert_eq!(
            cli.parallel_readers, 16,
            "Parallel readers should be parsed correctly"
        );
    }

    // Test 29: Chunk size validation - valid values
    #[test]
    fn test_chunk_size_validation_valid() {
        assert_eq!(
            parse_chunk_size_kb("4"),
            Ok(4),
            "Minimum chunk size should be valid"
        );
        assert_eq!(
            parse_chunk_size_kb("256"),
            Ok(256),
            "Default chunk size should be valid"
        );
        assert_eq!(
            parse_chunk_size_kb("1024"),
            Ok(1024),
            "Common chunk size should be valid"
        );
        assert_eq!(
            parse_chunk_size_kb("65536"),
            Ok(65536),
            "Maximum chunk size should be valid"
        );
    }

    // Test 30: Chunk size validation - invalid values
    #[test]
    fn test_chunk_size_validation_invalid() {
        // Too small
        let result = parse_chunk_size_kb("3");
        assert!(result.is_err(), "Chunk size < 4 should be invalid");
        assert!(
            result.unwrap_err().contains("at least 4 KB"),
            "Should mention minimum"
        );

        // Too large
        let result = parse_chunk_size_kb("65537");
        assert!(result.is_err(), "Chunk size > 64MB should be invalid");
        assert!(
            result.unwrap_err().contains("64 MB"),
            "Should mention maximum"
        );

        // Not a number
        let result = parse_chunk_size_kb("not_a_number");
        assert!(result.is_err(), "Non-numeric input should be invalid");
        assert!(
            result.unwrap_err().contains("not a valid number"),
            "Should mention invalid number"
        );
    }

    // Test 31: Parallel readers validation - valid values
    #[test]
    fn test_parallel_readers_validation_valid() {
        assert_eq!(
            parse_parallel_readers("0"),
            Ok(0),
            "Auto-detect should be valid"
        );
        assert_eq!(
            parse_parallel_readers("1"),
            Ok(1),
            "Sequential should be valid"
        );
        assert_eq!(
            parse_parallel_readers("8"),
            Ok(8),
            "Common reader count should be valid"
        );
        assert_eq!(
            parse_parallel_readers("64"),
            Ok(64),
            "Maximum readers should be valid"
        );
    }

    // Test 32: Parallel readers validation - invalid values
    #[test]
    fn test_parallel_readers_validation_invalid() {
        // Too many
        let result = parse_parallel_readers("65");
        assert!(result.is_err(), "Readers > 64 should be invalid");
        assert!(
            result.unwrap_err().contains("cannot exceed 64"),
            "Should mention maximum"
        );

        // Not a number
        let result = parse_parallel_readers("not_a_number");
        assert!(result.is_err(), "Non-numeric input should be invalid");
        assert!(
            result.unwrap_err().contains("not a valid number"),
            "Should mention invalid number"
        );
    }

    // Test 33: CLI parsing with invalid chunk size
    #[test]
    fn test_cli_parsing_invalid_chunk_size() {
        let args = vec![
            "checkle",
            "--chunk-size-kb",
            "2", // Too small
            "hash",
            "/tmp/test.txt",
        ];

        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Should fail with invalid chunk size");
    }

    // Test 34: CLI parsing with invalid parallel readers
    #[test]
    fn test_cli_parsing_invalid_parallel_readers() {
        let args = vec![
            "checkle",
            "--parallel-readers",
            "100", // Too many
            "hash",
            "/tmp/test.txt",
        ];

        let result = Cli::try_parse_from(args);
        assert!(result.is_err(), "Should fail with invalid parallel readers");
    }

    // Test 35: CLI defaults with no performance options
    #[test]
    fn test_cli_defaults_performance_options() {
        let args = vec!["checkle", "hash", "/tmp/test.txt"];

        let cli = Cli::try_parse_from(args).expect("Should parse with defaults");
        assert_eq!(cli.chunk_size_kb, 256, "Should default to 256KB chunks");
        assert_eq!(
            cli.parallel_readers, 0,
            "Should default to auto-detect readers"
        );
    }

    // Test 36: Performance test - CLI parsing time
    #[test]
    fn test_cli_parsing_performance() {
        use std::time::Instant;

        let args = vec![
            "checkle",
            "--algorithm",
            "sha256",
            "--threads",
            "4",
            "-vv",
            "verify",
            "/very/long/path/to/some/file/that/might/exist.txt",
            "--hash",
            "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
        ];

        // Warmup iteration to reduce cold-start effects
        let _ = Cli::try_parse_from(args.clone());

        let start = Instant::now();
        for _ in 0..1000 {
            let result = Cli::try_parse_from(args.clone());
            assert!(result.is_ok(), "Parsing should succeed in performance test");
        }
        let duration = start.elapsed();

        // CLI parsing should be fast - threshold adjusted for CI environment variability
        // Local development: stricter threshold, CI: more lenient due to shared resources
        let threshold_ms = if std::env::var("CI").is_ok() {
            600
        } else {
            400
        };
        assert!(
            duration.as_millis() < threshold_ms,
            "CLI parsing should be fast: {:?} (threshold: {}ms)",
            duration,
            threshold_ms
        );
    }

    // Property-based tests
    proptest! {
        #![proptest_config({
            Config {
                failure_persistence: Some(Box::new(
                    FileFailurePersistence::SourceParallel("tests/proptest-regressions")
                )),
                ..Default::default()
            }
        })]
        // Property 1: Valid algorithm strings always parse correctly
        #[test]
        fn test_valid_algorithm_parsing(algo in prop::sample::select(vec!["md5", "MD5", "sha2", "SHA2", "sha256", "SHA256", "sha-2", "sha-256"])) {
            let result = algo.parse::<HashingAlgo>();
            prop_assert!(result.is_ok());

            let parsed = result.unwrap();
            prop_assert!(matches!(parsed, HashingAlgo::Md5 | HashingAlgo::Sha2));
        }

        // Property 2: Case insensitive parsing for valid algorithms
        #[test]
        fn test_case_insensitive_algorithm_parsing(
            base_algo in prop::sample::select(vec!["md5", "sha2", "sha256"]),
            // Generate different case combinations
            case_pattern in prop::collection::vec(any::<bool>(), 3..6)
        ) {
            let mixed_case_algo = base_algo.chars().enumerate().map(|(i, c)| {
                if *case_pattern.get(i).unwrap_or(&false) {
                    c.to_uppercase().collect::<String>()
                } else {
                    c.to_lowercase().collect::<String>()
                }
            }).collect::<String>();

            let result = mixed_case_algo.parse::<HashingAlgo>();
            prop_assert!(result.is_ok());
        }

        // Property 3: Invalid algorithm strings always fail to parse
        #[test]
        fn test_invalid_algorithm_strings(
            invalid_algo in "([^m][^d][^5]|[^s][^h][^a])[a-z]{1,10}"
        ) {
            // Skip valid algorithm strings
            let lower = invalid_algo.to_lowercase();
            prop_assume!(!lower.starts_with("md5"));
            prop_assume!(!lower.starts_with("sha"));

            let result = invalid_algo.parse::<HashingAlgo>();
            prop_assert!(result.is_err());
        }

        // Property 4: Thread count parsing is robust for valid values
        #[test]
        fn test_thread_count_parsing_robustness(thread_count in 1usize..=1024) {
            let thread_count_str = thread_count.to_string();
            let args = vec![
                "checkle",
                "--threads", &thread_count_str,
                "hash",
                "/tmp/test.txt"
            ];

            let result = Cli::try_parse_from(args);
            prop_assert!(result.is_ok());

            let cli = result.unwrap();
            prop_assert_eq!(cli.threads, Some(thread_count));
        }


        // Property 6: Hash string validation for verify command
        #[test]
        fn test_hash_string_validation(
            hash in "[0-9a-fA-F]{32,64}"
        ) {
            let args = vec![
                "checkle",
                "verify",
                "/tmp/test.txt",
                "--hash", &hash
            ];

            let result = Cli::try_parse_from(args);
            prop_assert!(result.is_ok());

            if let Ok(cli) = result {
                if let Some(Commands::Verify { hash: parsed_hash, .. }) = cli.command {
                    prop_assert_eq!(parsed_hash, Some(hash));
                }
            }
        }

        // Property 7: Chunk size validation property test
        #[test]
        fn test_chunk_size_property_validation(
            chunk_size in 4usize..=65536
        ) {
            let chunk_size_str = chunk_size.to_string();
            let result = parse_chunk_size_kb(&chunk_size_str);
            prop_assert!(result.is_ok());
            prop_assert_eq!(result.unwrap(), chunk_size);
        }

        // Property 8: Parallel readers validation property test
        #[test]
        fn test_parallel_readers_property_validation(
            readers in 0usize..=64
        ) {
            let readers_str = readers.to_string();
            let result = parse_parallel_readers(&readers_str);
            prop_assert!(result.is_ok());
            prop_assert_eq!(result.unwrap(), readers);
        }

        // Property 9: CLI with performance options property test
        #[test]
        fn test_cli_performance_options_property(
            chunk_size in 4usize..=65536,
            readers in 0usize..=64
        ) {
            let chunk_size_str = chunk_size.to_string();
            let readers_str = readers.to_string();
            let args = vec![
                "checkle",
                "--chunk-size-kb", &chunk_size_str,
                "--parallel-readers", &readers_str,
                "hash",
                "/tmp/test.txt"
            ];

            let result = Cli::try_parse_from(args);
            prop_assert!(result.is_ok());

            if let Ok(cli) = result {
                prop_assert_eq!(cli.chunk_size_kb, chunk_size);
                prop_assert_eq!(cli.parallel_readers, readers);
            }
        }
    }
}
