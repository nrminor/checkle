# Command Line Usage

Complete reference for checkle's command-line interface.

## Global Options

```bash
checkle [OPTIONS] <COMMAND>
```

Options available for all commands:
- `-v, --verbose`: Increase logging verbosity (use multiple times)
- `-q, --quiet`: Suppress non-essential output
- `--version`: Display version information
- `--help`: Show help information

## Commands

### hash

Generate checksums for files.

```bash
checkle hash [OPTIONS] <FILE_OR_DIR>
```

**Options:**
- `--algo <ALGORITHM>`: Hash algorithm (md5, sha256) [default: md5]
- `-r, --recursive`: Process directories recursively
- `-o, --output <FILE>`: Save checksums to file
- `--format <FORMAT>`: Output format (text, json, csv)
- `--pretty`: Display results in a formatted table
- `--per-file`: Create individual checksum files
- `--include <PATTERN>`: Include only matching files
- `--exclude <PATTERN>`: Exclude matching files
- `--no-ignore`: Don't respect .gitignore rules
- `--parallel-readers <N>`: Number of parallel readers [default: auto]
- `--chunk-size-kb <SIZE>`: Chunk size in KB [default: 1024]
- `--absolute-paths`: Display absolute paths in output

**Examples:**
```bash
# Hash single file
checkle hash file.txt

# Hash directory recursively
checkle hash /data --recursive

# Save SHA-256 checksums to file
checkle hash *.fastq --algo sha256 -o checksums.sha256

# Pretty table output
checkle hash *.bam --pretty
```

### verify

Verify a file against a known hash.

```bash
checkle verify [OPTIONS] <FILE> --hash <HASH>
```

**Options:**
- `--hash <HASH>`: Expected hash value (required)
- `--algo <ALGORITHM>`: Hash algorithm [default: md5]
- `--chunk-size-kb <SIZE>`: Chunk size in KB
- `--parallel-readers <N>`: Number of parallel readers

**Examples:**
```bash
# Verify MD5
checkle verify genome.fasta --hash d41d8cd98f00b204e9800998ecf8427e

# Verify SHA-256
checkle verify data.tar.gz --hash e3b0c44298fc1c149afbf4c8996fb924 --algo sha256
```

### verify-many

Verify multiple files using a checksum file.

```bash
checkle verify-many [OPTIONS] --checksum-file <FILE>
```

**Options:**
- `--checksum-file <FILE>`: File containing checksums (required)
- `--base-dir <DIR>`: Base directory for relative paths
- `--algo <ALGORITHM>`: Hash algorithm [default: md5]
- `--fail-fast`: Stop on first verification failure
- `--quiet`: Only show failures
- `--pretty`: Display results in formatted table
- `--parallel-files <N>`: Files to process in parallel
- `--chunk-size-kb <SIZE>`: Chunk size in KB
- `--failed-only`: Only show failed verifications

**Examples:**
```bash
# Verify all files in checksum list
checkle verify-many --checksum-file checksums.txt

# Stop on first failure
checkle verify-many --checksum-file checksums.txt --fail-fast

# Show only failures
checkle verify-many --checksum-file checksums.txt --failed-only

# Verify with custom base directory
checkle verify-many --checksum-file checksums.txt --base-dir /data
```

## Archive Syntax

Access files within archives using colon notation:

```bash
# Hash specific file in archive
checkle hash archive.tar:path/to/file.txt

# Hash all files matching pattern
checkle hash archive.zip:*.csv

# Hash all files in archive
checkle hash archive.tar.gz:*
```

Supported archive formats:
- TAR (.tar, .tar.gz, .tar.bz2, .tar.xz)
- ZIP (.zip)

## Pattern Matching

Use glob patterns to filter files:

```bash
# Include patterns
checkle hash /data --include "*.fastq" --include "*.fasta"

# Exclude patterns
checkle hash /data --exclude "*.tmp" --exclude ".git/**"

# Combine include and exclude
checkle hash /data --include "*.txt" --exclude "*test*"
```

## Exit Codes

- `0`: Success
- `1`: General error
- `2`: Verification failure
- `3`: File not found
- `4`: Invalid arguments