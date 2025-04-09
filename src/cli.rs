use std::{path::PathBuf, str::FromStr};

use crate::hashing::HashingAlgo;
use clap::{
    builder::{TypedValueParser, ValueParserFactory},
    error::ErrorKind,
    Error, Id, Parser, Subcommand,
};

pub const INFO: &str = r#"
   ___    _  _     ___     ___    _  __    _       ___   
  / __|  | || |   | __|   / __|  | |/ /   | |     | __|  
 | (__   | __ |   | _|   | (__   | ' <    | |__   | _|   
  \___|  |_||_|   |___|   \___|  |_|\_\   |____|  |___|  
_|"""""|_|"""""|_|"""""|_|"""""|_|"""""|_|"""""|_|"""""| 
"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-'"`-0-0-' 

checkle (v0.1.0)
------------------------------------------------------------
A `checksum` utility for the multicore age. `checkle` implements a Merkle hash
digest tree with parallelized nodes to progressively hash large files in small
chunks. It can be used to hash single files, hash lists of files, and generate
new hashes for files to be checked later. Currently, it supports MD5 and SHA256.
"#;
pub const VERSION: &str = "v0.1.0";

#[derive(Parser)]
#[clap(name = "checkle")]
#[clap(about = INFO)]
#[clap(version = VERSION)]
pub struct Cli {
    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Number of parallel threads to use when computing the checksum
    #[arg(short = 'j', long, required = false)]
    pub threads: Option<usize>,

    /// The hashing algorithm to use
    #[arg(short, long, required = false, default_value = "md5", value_parser = HashingAlgo::value_parser())]
    pub algorithm: HashingAlgo,
}

#[derive(Subcommand)]
pub enum Commands {
    #[clap(
        about = "Verify a file with the provided hash.",
        visible_aliases = &["check", "checksum", "c", "ver", "verfy", "verfiy", "v"]
    )]
    Verify {
        /// Input file to checksum
        #[arg(index = 1, required = true)]
        input_file: PathBuf,

        /// Expected hash for the input file
        #[arg(short, long, required = true)]
        hash: String,
    },

    #[clap(
            about = "Perform checksums on all the files in a provided text file",
            visible_aliases = &["all", "many", "list", "a", "m", "queue", "q"]
        )]
    VerifyMany {
        /// Tab-delimited text file listing file name in the first column and checksum in the second
        #[arg(short, long, required = true)]
        checksum_file: PathBuf,
    },

    #[clap(
            about = "Compute hashes for any input file(s) that can be checksummed later.",
            visible_aliases = &["h", "n", "init", "new", "g", "gen", "generate"]
        )]
    Hash {
        /// Input file to checksum
        #[arg(index = 1, required = true, default_value = "./*")]
        input_file: PathBuf,
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
