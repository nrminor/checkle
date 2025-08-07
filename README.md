# checkle: Extremely fast checksum runner for arbitrarily large batches of large files

[![CI (Stable)](https://github.com/nrminor/checkle/workflows/CI%20(Stable)/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/ci-stable.yml)
[![CI (SIMD Nightly)](https://github.com/nrminor/checkle/workflows/CI%20(SIMD%20Nightly)/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/ci-simd.yml)
[![Security Audit](https://github.com/nrminor/checkle/workflows/Security%20Audit/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/audit.yml)
[![Release](https://github.com/nrminor/checkle/workflows/Release/badge.svg)](https://github.com/nrminor/checkle/actions/workflows/release.yml)
[![Documentation](https://img.shields.io/badge/docs-mdbook-blue)](https://nrminor.github.io/checkle/)

A `checksum` utility for the multicore era.

> [!WARNING]
> `checkle` is an interesting but ultimately abandoned prototype. It rapidly
> performs hashes and has an excellent UX, but for theoretical reasons, it is
> guaranteed to produces different hashes for large files than canonical tools
> like `md5sum` do. In an attempt to become a parallelized Merkle tree
> implementation with MD5 and SHA256, `checkle` in some sense became its own
> hashing algorithm. It can thus be used on its own like `md5sum`, e.g., hashing
> a file on a source endpoint and verifying its transfer on a destination
> endpoint, but on both endpoints, hashes will not match hashes produced by
> `md5sum` or `sha256sum`.
>
> All this is to say, use at your own risk! Despite abandoning this prototype,
> we learned a lot along the way, and viewers should feel free to peruse the
> codebase for examples of how to do various things in Rust (e.g., SIMD
> optimizations, features, `rayon` parallelism, property testing, etc.), how to
> use justfiles to manage projects, how to set up Rust `mdbook` documentation
> and CI/CD with github workflows, and how to corrale coding agents.
>
> Below is the original README. Please see
> [this document](/context/CRITICAL_MERKLE_TREE_FAILURE_POSTMORTEM.md) for a detailed
> post-mortem on the project. 

### Overview

I work in genomics. This means I often transfer batches of data files from
sequencing cores, where each file can be as much as a half-a-terabyte. Checking
the integrity of these files post-transfer can be an arduous, time-consuming
task. To run checksums, it's not unusual to stay in one's scripting comfort
zone, write a shell or Python for loop, and run `md5sum` or some other
single-threaded utility on a file in each iteration, waiting however long it
takes for the serial integrity checks to finish. This process can be agonizingly
slow, and without good reason: modern CPUs come with the ability to spread
computations across cores and use "wide" SIMD operations on each. Traditional
checksum utilities also leave some additional optimizations made possible by SSD
storage on the table. We can do better and get to the fun part--analyzing data
and doing science--faster.

`checkle` aims to make slow, serial checksums obsolete and bring file integrity
checks into the multicore era. It performs checksums on batches of files using
[Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree) and
[SIMD](https://en.wikipedia.org/wiki/Single_instruction,_multiple_data) to
parallelize and accelerate hashing. It also comes with a variety of
quality-of-life features including progress bars, customizable recursive
directory traversal, multiple report formats, sophisticated logging, and more.

### Features

- Equivalent but modernized user experience to `md5sum` or `sha2sum`
- Each file is hashed in parallel chunks; in total, the time to check a single
  file will be close to a function of the file size divided by your number of
  cores. ~~And unlike other performant file integrity checkers, you don't need
  to switch to a new hashing algorithm to benefit from this multicore
  speedup--your legacy md5 or sha2 checksum files are fully compatible with
  `checkle`.~~
- Many files throughout a file hierarchy can be hashed or checked recursively,
  with optional include or exclude filters to hash/verify just the files you're
  interested in
- (Unfinished!) Support for running checks on files _within_ TAR and ZIP archives without
  extracting and decompressing files from them
- Check successes and failures can be pretty-printed to standard output or
  written as CSV or JSON for your post-processing convenience
- Multiple hashing algorithms including SHA2 and MD5 are supported, with support
  for more algorithms, e.g. BLAKE3, planned for the future
- Sophisticated logging, helpful error messages, and a full test suite

### Installation

#### Quick Install (Recommended)

```bash
# Standard build
curl -fsSL https://raw.githubusercontent.com/nrminor/checkle/main/INSTALL.sh | sh

# SIMD-optimized build (faster, requires modern CPU)
curl -fsSL https://raw.githubusercontent.com/nrminor/checkle/main/INSTALL.sh | sh -s -- --simd
```

#### Manual Binary Download

Download from [releases](https://github.com/nrminor/checkle/releases):

```bash
# SIMD-optimized (recommended for modern CPUs)
wget https://github.com/nrminor/checkle/releases/latest/download/checkle-x86_64-unknown-linux-gnu-simd.tar.gz
tar -xzf checkle-x86_64-unknown-linux-gnu-simd.tar.gz
sudo mv checkle /usr/local/bin/

# Standard compatibility version
wget https://github.com/nrminor/checkle/releases/latest/download/checkle-x86_64-unknown-linux-gnu.tar.gz
```

#### Cargo Install

```bash
# From crates.io (when published)
cargo install checkle

# With cargo-binstall (if available)
cargo binstall checkle

# From source
cargo install --git https://github.com/nrminor/checkle
```

### Basic Usage

#### Regular File Operations

```bash
# Hash a single file
checkle hash myfile.txt

# Verify a file against a known hash
checkle verify myfile.txt --hash abcdef1234567890abcdef1234567890

# Batch verify multiple files from a checksum file
checkle verify-many --checksum-file checksums.md5
```

#### Archive Support

checkle provides comprehensive support for TAR (.tar, .tar.gz, .tar.bz2,
.tar.xz, .tgz) and ZIP archives:

```bash
# Hash a specific file within an archive
checkle hash archive.tar:path/to/file.txt

# Verify a specific file within an archive
checkle verify archive.tar:file.txt --hash abcdef1234567890abcdef1234567890

# Hash all files within an archive (archive traversal)
checkle hash archive.tar --recursive

# Verify files referenced from within archives using checksum file
checkle verify-many --checksum-file checksums_in_archive.md5
# Where checksums_in_archive.md5 contains lines like:
# abcdef1234567890  archive.tar:file1.txt
# 1234567890abcdef  archive.tar:subdir/file2.txt
```

#### Advanced Options

```bash
# Use SHA256 instead of MD5
checkle hash myfile.txt --algorithm sha256

# Recursive directory hashing with pattern filtering
checkle hash /path/to/dir --recursive --include "*.fastq.gz"

# Pretty formatted output
checkle hash myfile.txt --pretty
```

### Testing and Contributing

`checkle` includes comprehensive test suites to ensure correctness and
performance. There are three types of tests:

#### 1. Unit Tests

Run the standard Rust unit tests:

```bash
# Run all tests
cargo test

# Run tests with output displayed
cargo test -- --nocapture

# Run a specific test
cargo test test_md5_hasher_normal_operation
```

#### 2. Hash Verification Tests

These tests verify that `checkle` produces identical hashes to well-established
utilities like `md5sum` and `sha256sum`:

```bash
# Run the hash verification script
./tests/verify_hashes.sh

# Or use the justfile command
just verify-hashes

# Run all tests including verification
just test-all
```

The verification tests will:

- Build checkle in release mode
- Compare MD5 and SHA256 hashes against standard system utilities
- Test both single file and batch verification modes
- Generate a detailed report in `tests/results/`

#### 3. Property-Based Tests

The test suite includes property-based tests using `proptest` to ensure
algorithmic correctness across a wide range of inputs:

- Hash determinism (same input always produces same hash)
- Hash length invariants (MD5 always 32 chars, SHA256 always 64 chars)
- Input validation properties

### Benchmarking

To measure performance and compare against standard utilities:

#### Running Benchmarks

```bash
# Run the comprehensive benchmark suite
./benches/benchmark_checksums.sh

# Or use justfile commands
just benchmark-md5 test-file.bin
just benchmark-sha256 test-file.bin
```

The benchmark script will:

- Create test files of various sizes (1MB to 5GB)
- Compare checkle against md5sum, sha256sum, rhash, xxhsum, and b3sum
- Use hyperfine for accurate measurements with warmup runs
- Generate detailed reports with performance comparisons

#### Creating Test Files

To create test files for benchmarking:

```bash
# Create standard test files (1MB, 100MB, 1GB)
just create-test-files

# Or create custom sizes manually
dd if=/dev/urandom of=test-10gb.bin bs=1M count=10240
```

#### Interpreting Results

The benchmarks demonstrate checkle's performance advantages:

- Performance scales with file size and available CPU cores
- Merkle tree parallelization provides significant speedup on multicore systems
- Most beneficial for large files (>100MB) common in bioinformatics

### Development Workflow

The project includes a comprehensive `justfile` with development commands:

```bash
# Show all available commands
just

# Quality checks (format, lint, test) - REQUIRED before commits
just check

# Run clippy with strict lints
just clippy

# Format code
just fmt

# Build in release mode
just release

# Install locally
just install
```

### Building with SIMD Optimizations

checkle includes optional SIMD optimizations for improved performance. These
require Rust nightly:

```bash
# Install nightly toolchain if needed
rustup toolchain install nightly

# Build with SIMD optimizations
cargo +nightly build --release --features simd

# Build with native CPU optimizations for maximum performance
RUSTFLAGS="-C target-cpu=native" cargo +nightly build --release --features simd

# Run SIMD tests
just test-simd

# Run SIMD benchmarks
just bench-simd
```

### Testing Requirements

Before submitting any changes:

1. All tests must pass: `just test`
2. Hash verification must pass: `just verify-hashes`
3. Code must be formatted: `just fmt`
4. Clippy must report zero warnings: `just clippy`

The project enforces strict quality standards with comprehensive linting rules.

### Rust Version Requirements

- **Minimum supported Rust version**: 1.88.0
- **For SIMD features**: Rust nightly (for `portable_simd`)

### Working with AI Agents

This codebase is designed to provide amble context, including goals, best
practices, and strict rules, to work effectively with AI agents and assistants
with vary context window sizes. These rules address code quality standards
varying from project style considerations to critical performance and
correctness considerations.

#### Strict Rule Compliance

Most importantly, AI agents must follow all development rules without exception,
including:

- Run `cargo fmt`, `cargo check`, and
  `cargo clippy --all-targets --all-features -- -D warnings` before declaring
  any work complete
- Never add `#[allow()]` lint suppressions without explicit permission
- Follow the Three-Test Rule: every change must include at least 3 tests
- Each change must be carefully considered with respect to how its value
  compares to the maintenance burden or "entropy" it introduces to the code base

#### Frequent Context Loading

Before making any changes, AI agents must read and understand:

1. **AGENTS.md** - Complete development guidelines and project rules
2. **README.md** - Project overview and goals
3. **TIGER_STYLE.md** - Robustness principles
4. **GRUG BRAIN DEVELOPER** - Simplicity principles

#### Essential Guidelines for AI Agents

- **Read AGENTS.md first**: This document contains comprehensive instructions
  for working on the codebase
- **Strict quality standards**: The project enforces zero clippy warnings and
  comprehensive testing
- **Performance focus**: Always consider multicore utilization and large file
  handling
- **Simple solutions**: Balance robustness with simplicity following Grug Brain
  principles
- **Bioinformatics context**: Remember this tool handles terabyte-scale genomics
  files

Working without reading these documents is unacceptable and will result in code
that doesn't meet project standards.
