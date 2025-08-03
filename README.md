# checkle: Extremely fast checksum runner for arbitrarily large batches of large files

A `checksum` utility for the multicore era. It's so fast it will make you
chuckle.

### Overview

I work in genomics. This means I often transfer handfuls of files from
sequencing cores, where each file can be as much as a half-a-terabyte. As such,
checking the integrity of these files post-transfer can be an arduous,
time-consuming task. To run checksums, it's not unusual to stay in one's
scripting comfort zone, write a shell or Python for loop, and run `checksum` or
some other single-threaded utility on a file in each iteration, waiting however
long it takes for the serial integrity checks to finish. This process can be
agonizingly slow, and without good reason: modern CPUs come with the ability to
spread computations across cores and use "wide" SIMD operations on each.
Traditional checksum utilities also leave some additional optimizations made
possible by SSD storage on the table. We can do better and get to the fun
part--analyzing data and doing science--faster.

`checkle` aims to make slow, serial checksums obsolete and bring file integrity
checks into the multicore era. It performs parallelized checksums on batches of
files transferred over the interwebs, using
[Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree) and
[SIMD](https://en.wikipedia.org/wiki/Single_instruction,_multiple_data) to
accelerate hashing on multicore machines.

### Features

- Equivalent but modernized user experience to `checksum`
- Each file is hashed in parallel chunks; in total, the time to check a single
  file will be close to a function of the file size divided by your number of
  cores
- Many files throughout a file hierachically can be hashed or checked
  recursively, with optional include or exclude filters to hash just the files
  you're interested in
- Support for running checks on files _within_ TAR and ZIP archives without
  extracting and decompressing files from them (WIP!)
- Check successes and failures can be pretty-printed to standard output or
  written as CSV or JSON
- Multiple hashing algorithms including SHA2 and MD5 are supported

### Development Goals

I have the following goals for `checkle`:

- [x] Spread hashing across as many (virtual) cores as possible using
      [Merkle Trees](https://en.wikipedia.org/wiki/Merkle_tree) (for the heads:
      `checkle` is a portmanteau of checksum and Merkle).
- [x] If a manifest of hashes from the source server is provided, spread
      post-transfer checksums across cores as well.
- [x] Support md5 for backward compatibility along with at least one more
      cryptographically secure hashing function.
- [ ] Be capable of reaching into `tar` and `zip` archives to checksum files
      without decompressing the whole archive.
- [x] Have an easy-to-use command line interface powered by
      [`clap`](https://docs.rs/clap/latest/clap/).
- [ ] Be easy to install through [crates.io](https://crates.io/) as well as with
      binaries for your platform of choice distributed in this repo.
- [x] Pretty-print a report to `stderr` on which files should be re-transferred.

`checkle` will be made available on [crates.io](https://crates.io/) when it
reaches a reasonable level of stability.

### Testing

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

### Testing Requirements

Before submitting any changes:

1. All tests must pass: `just test`
2. Hash verification must pass: `just verify-hashes`
3. Code must be formatted: `just fmt`
4. Clippy must report zero warnings: `just clippy`

The project enforces strict quality standards with comprehensive linting rules.
