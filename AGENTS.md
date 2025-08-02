# AGENTS.md - Comprehensive AI Assistant Instructions for checkle

This document consolidates all AI assistant instructions for the checkle
project into a single comprehensive reference.

## MANDATORY FIRST STEP: Document Review

**CRITICAL**: Before performing ANY work on this codebase, you MUST read and
understand ALL of the following documents:

1. **AGENTS.md** (this document) - Complete development guidelines and rules
2. **README.md** - Project overview and goals
3. **TIGER_STYLE.md** - World-class software robustness principles (if not
   available locally, read from
   https://raw.githubusercontent.com/tigerbeetle/tigerbeetle/refs/heads/main/docs/TIGER_STYLE.md)

**Work performed without reviewing these documents is UNACCEPTABLE.** These
documents contain essential information about project goals, coding standards,
and architectural decisions.

## Project: checkle

An extremely fast checksum utility for the multicore age, designed for
bioinformatics workflows involving arbitrarily large batches of large files.

### Quick Summary

checkle provides blazing-fast file integrity verification using Merkle trees to
parallelize hashing across all available CPU cores. It's particularly suited for
genomics workflows where files can be hundreds of gigabytes each.

### Core Functionality

- **Hash single files**: `checkle hash <file>` - Generate Merkle tree hash
- **Verify single files**: `checkle verify <file> --hash <hash>` - Check
  integrity
- **Verify many files**: `checkle verify-many --checksum-file <file>` - Batch
  verification
- **Parallel processing**: Merkle tree implementation for multicore speedup
- **Multiple algorithms**: MD5 (fast, backward-compatible) and SHA256 (secure)

### Technology Stack

- **Language**: Rust (latest stable)
- **Architecture**: Parallel processing with rayon
- **Hashing**: Merkle trees for parallelization
- **Error Handling**: color-eyre for main, custom errors elsewhere
- **CLI**: clap with custom styling

### Project Structure

```
src/              # Rust source code
  main.rs         # Entry point and CLI orchestration
  cli.rs          # Command-line interface definitions
  hashing.rs      # Merkle tree and hashing implementation
  io.rs           # File I/O and batch processing
  prelude.rs      # Common imports and error types
  lib.rs          # Library root
Cargo.toml        # Rust dependencies
README.md         # Project overview
justfile          # Development commands
```

### Key Design Principles

1. **Performance First**: Maximize multicore utilization for large files
2. **Bioinformatics Focus**: Handle terabyte-scale genomics files efficiently
3. **Simple CLI**: Git-like commands that are intuitive
4. **Reliability**: Robust error handling for production use
5. **Extensibility**: Modular design for future hash algorithms

### Design Goals (in order)

1. **Speed** - Merkle trees for parallel hashing, minimal allocations
2. **Correctness** - Accurate checksums, proper error handling
3. **Usability** - Clear CLI, helpful error messages, progress indicators

## CRITICAL: Development Rules

### Code Verification Rule (NON-NEGOTIABLE)

**It is ABSOLUTELY FORBIDDEN to declare any Rust code as finished, complete, or
ready for review without FIRST running:**

1. `cargo fmt` - Must be run to ensure consistent code formatting
2. `cargo check` - Must pass with zero errors
3. `cargo clippy --all-targets --all-features -- -D warnings` - Must pass with
   ZERO warnings

**CRITICAL**: This project uses strict clippy lints defined in files:

- `clippy::all`
- `clippy::pedantic`
- `clippy::perf`
- `clippy::style`
- `clippy::complexity`
- `clippy::correctness`
- `clippy::unwrap_used`

**NO EXCEPTIONS.** Any claim of completion without these checks is unacceptable.

### The Three-Test Rule (MANDATORY)

**MANDATORY FOR ALL CODE CHANGES**: Every update to ANY part of the codebase
MUST include AT LEAST THREE tests:

1. **Add** three new tests for new functionality, OR
2. **Replace** three existing tests with better tests, OR
3. **Improve** three existing tests to be more comprehensive

**NO EXCEPTIONS.** This rule applies to all changes.

### Tiger Style Requirements

For EVERY development task:

1. Review TIGER_STYLE.md principles
2. Implement at least THREE improvements based on Tiger Style
3. Document which Tiger Style principles were applied

Key principles to apply:

- **Assertions**: Minimum 2 assertions per function (pre/postconditions)
- **Resource Limits**: Put a limit on everything (file sizes, chunk counts,
  etc.)
- **Function Size**: Soft limit of 70 lines per function
- **Positive Invariants**: Check what should be true, not what shouldn't

### Entropy Awareness

**ENTROPY THRESHOLD PRINCIPLE**: Every diff must be evaluated against its
entropy cost:

**ENTROPY EVALUATION CRITERIA**:

- **Ideal**: Entropy increase is **less than proportional** to value delivered
- **Acceptable**: Entropy increase is **proportional** to value delivered
- **Unacceptable**: Entropy increase **exceeds** value delivered

**PRINCIPLE**: Every line of code is a liability. Maximize value while
minimizing entropy increase.

## Error Handling Philosophy

**Every error MUST include:**

1. What went wrong (specific)
2. Context (file paths, sizes, hash values)
3. Why it might have happened
4. How to fix it (actionable steps)

Example:
`cannot read file {path}: {err}\n\nPossible causes:\n1. File does not exist\n2. Insufficient permissions\n3. File is locked by another process`

## Performance Principles

- **Design for performance upfront** - Merkle trees chosen for parallelization
- **Measure and optimize** - Profile large file operations
- **Minimize allocations** - Reuse buffers where possible
- **Exploit parallelism** - Use all available cores effectively
- **Batch operations** - Process multiple files concurrently

## Function Design Principles

**Prefer Pure Functions**

- Functions should avoid side effects unless necessary
- Pure functions are preferred for hashing operations
- I/O operations should be clearly isolated

**Reduce Nesting with Early Returns**

- Use early returns for error cases
- Use `let-else` pattern to avoid nested if-let pyramids
- Keep the happy path at the lowest indentation level

## Testing Philosophy

**PRINCIPLE: Test outcomes, not implementation details**

We do not subscribe to unit test absolutism or TDD dogma. Every test must provide real value by verifying observable behavior and outcomes, not internal implementation details.

### Core Testing Principles

1. **Test Behavior, Not Assertions**
   - Don't write tests that duplicate precondition/postcondition assertions
   - If production code has `assert!(file.exists())`, don't test that assertion
   - Test what the code DOES, not how it validates inputs

2. **Tests Must Be Fast**
   - Limit temporary file creation (max ~20 files per test)
   - Property tests should use `.with_cases(5-10)` to limit iterations
   - Avoid tests that take more than 1 second

3. **Avoid Implementation Coupling**
   - Tests shouldn't break when internal implementation changes
   - Don't test private methods or internal state
   - Focus on public API behavior

4. **Remove Redundant Tests**
   - If an assertion already enforces a constraint, don't test it
   - One good test is better than five redundant ones
   - Delete tests that only verify what assertions already check

5. **Test Real Scenarios**
   - Use realistic file sizes and content
   - Test actual hash values for known inputs
   - Verify real checksum verification scenarios

### What NOT to Test

- Precondition/postcondition assertions
- Getter/setter behavior without logic
- Implementation details that might change
- Error messages exact wording
- Debug formatting details

### What TO Test

- Core algorithms produce correct results
- File parsing handles various formats
- Hash verification catches mismatches
- Performance characteristics are maintained
- Edge cases that affect user-visible behavior

## Documentation Standards

- **Always explain WHY** - especially for performance decisions
- Document Big-O complexity for key algorithms
- Include benchmarks in documentation
- Keep examples realistic for bioinformatics use cases

## Code Style Guidelines

- Follow Rust idioms and conventions
- Use rayon's parallel iterators effectively
- **Error Handling**: Custom error types with thiserror in modules, color-eyre
  in main
- Keep functions focused and under 70 lines
- Use descriptive names that reflect bioinformatics domain
- **NO EMOJIS**: Do not use emojis in code, comments, documentation, or commit messages

### Self-Documenting Types (Soft Rule)

**Use the type system for documentation and correctness**: The type system
should be used not just to verify correctness, but also to make
"self-documenting code." Types like `String` and `bool`, especially in function
signatures, can very often represent almost anything. In cases where it makes
sense, `String` and `bool` should be replaced with custom types and type aliases
that can handle parsing information from naive strings and also function as a
form of documentation for what the `String` or `bool` represents.

**Examples of Self-Documenting Types**:

```rust
// Instead of: fn process_file(path: String, size_limit: u64, verify: bool) -> Result<()>
// Use custom types that leverage Rust's zero-cost abstractions:

#[derive(Debug, Clone)]
pub struct FilePath(PathBuf);

impl FilePath {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        if path.exists() {
            Ok(Self(path))
        } else {
            Err(Error::file_not_found(path))
        }
    }
    
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FileSizeLimit(u64);

impl FileSizeLimit {
    pub fn new(bytes: u64) -> Result<Self, Error> {
        const MAX_FILE_SIZE: u64 = 1_000_000_000_000; // 1TB
        if bytes > 0 && bytes <= MAX_FILE_SIZE {
            Ok(Self(bytes))
        } else {
            Err(Error::invalid_file_size(bytes))
        }
    }
    
    pub fn as_bytes(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VerificationMode {
    Strict,
    Lenient,
    SkipVerification,
}

// Now the function signature is self-documenting:
fn process_file(path: FilePath, limit: FileSizeLimit, mode: VerificationMode) -> Result<()> {
    // Implementation benefits from validated inputs
    // and clear intent from the type system
}
```

**Another Example - Configuration Flags**:

```rust
// Instead of: fn hash_files(recursive: bool, follow_links: bool, verbose: bool)
// Use descriptive type aliases for simple flags:

pub type Recursive = bool;
pub type FollowSymlinks = bool;
pub type VerboseOutput = bool;

// Clear, self-documenting function signature:
fn hash_files(
    recursive: Recursive,
    follow_links: FollowSymlinks,
    verbose: VerboseOutput
) -> Result<()> {
    if recursive {
        // Recursively process directories
    }
    
    if follow_links {
        // Follow symbolic links
    }
    
    if verbose {
        println!("Processing files...");
    }
    
    // Hash files...
    Ok(())
}

// Usage is simple and clear:
hash_files(
    true,  // recursive: Recursive
    false, // follow_links: FollowSymlinks  
    true   // verbose: VerboseOutput
)?;
```

**Note**: For simple boolean flags, type aliases provide documentation benefits without the overhead of wrapper types. Reserve newtype patterns for cases where you need:
- Validation logic
- Multiple representations of the same underlying type
- Prevention of mixing different semantic meanings
- Methods specific to that type

**Benefits of this approach**:

- **Zero runtime cost**: These are zero-cost abstractions that compile to the
  same machine code
- **Self-documenting**: Function signatures clearly communicate intent
- **Validation at construction**: Invalid values are caught early
- **Type safety**: Prevents mixing up parameters (can't pass timeout where path
  expected)
- **IDE support**: Better autocomplete and error messages

### Struct Methods for Complex Functions (Soft Rule)

**When a function has enough parameters to trigger a clippy warning, it should
be refactored into a method on a struct**. This pattern leverages Rust's
zero-cost abstractions to create cleaner, more maintainable code.

**Benefits of the Struct Method Pattern**:

- **Simplifies function signatures**: Related parameters are grouped logically
- **Self-documenting**: The struct name and field names provide context
- **Easier testing**: Test data can be built incrementally with struct literals
- **API cleanliness**: Implementation details can be kept private
- **Idiomatic Rust**: Aligns with Rust's emphasis on type-driven design

**Example Refactoring**:

```rust
// Before: Function with many parameters (triggers clippy::too_many_arguments)
fn verify_checksums(
    checksum_file: &Path,
    base_dir: &Path,
    algorithm: HashingAlgo,
    parallel_files: usize,
    fail_fast: bool,
    quiet_mode: bool,
    show_progress: bool,
    max_errors: usize,
) -> Result<VerificationResults> {
    // Implementation
}

// After: Method on a configuration struct
#[derive(Debug, Clone)]
pub struct VerificationConfig {
    pub checksum_file: PathBuf,
    pub base_dir: PathBuf,
    pub algorithm: HashingAlgo,
    pub parallelism: ParallelismConfig,
    pub output_options: OutputOptions,
    pub error_handling: ErrorHandling,
}

#[derive(Debug, Clone)]
pub struct ParallelismConfig {
    pub max_files: usize,
}

#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub quiet: bool,
    pub show_progress: bool,
}

#[derive(Debug, Clone)]
pub struct ErrorHandling {
    pub fail_fast: bool,
    pub max_errors: usize,
}

impl VerificationConfig {
    /// Create a new verification configuration with sensible defaults
    pub fn new(checksum_file: PathBuf, base_dir: PathBuf) -> Self {
        Self {
            checksum_file,
            base_dir,
            algorithm: HashingAlgo::default(),
            parallelism: ParallelismConfig { max_files: 4 },
            output_options: OutputOptions {
                quiet: false,
                show_progress: true,
            },
            error_handling: ErrorHandling {
                fail_fast: false,
                max_errors: 10,
            },
        }
    }
    
    /// Execute the verification with this configuration
    pub fn verify(&self) -> Result<VerificationResults> {
        // Implementation can access all fields via self
        validate_checksum_file(&self.checksum_file)?;
        let hasher = create_hasher(&self.algorithm)?;
        // ...
    }
}

// Usage becomes cleaner and more flexible:
let config = VerificationConfig::new(checksum_file, base_dir)
    .with_algorithm(HashingAlgo::SHA256)
    .with_parallelism(8);

let results = config.verify()?;
```

**Testing Benefits**:

```rust
#[test]
fn test_verification_with_errors() {
    let config = VerificationConfig {
        error_handling: ErrorHandling {
            fail_fast: true,
            max_errors: 1,
        },
        ..Default::default()  // Only specify what's relevant to the test
    };
    // Test is focused and clear
}
```

This pattern is particularly valuable in this codebase where we emphasize
self-documenting code and type safety. It also helps manage the complexity that
naturally emerges in a feature-rich CLI tool.

### Checkle-Specific Conventions

- **Hash Algorithms**: Extensible enum design for future algorithms
- **Chunk Size**: 1MB chunks for optimal performance (const CHUNK_SIZE)
- **Merkle Trees**: Recursive implementation with parallel processing
- **File Collections**: Custom types for batch operations

## Dependency Management

### Zero Unnecessary Dependencies Policy

**CRITICAL**: This project follows a strict dependency management policy:

1. **Always ask before adding dependencies** - Every new dependency must be approved by the user before adding to Cargo.toml
2. **Justify every dependency** - Each dependency must have a clear, documented reason for inclusion
3. **Prefer standard library** - Use std over external crates when possible
4. **Minimal footprint** - The project must not become bloated with unnecessary dependencies
5. **Security conscious** - All dependencies must be actively maintained and security-audited

### Current Dependencies and Justifications

#### Core Dependencies
- **clap** (4.4.3) - CLI argument parsing with derive macros. Essential for user-friendly command-line interface
- **clap-verbosity-flag** (2.1.1) - Standard verbosity flags (-v, -vv, etc). Provides consistent logging control
- **color-eyre** (0.6.3) - Error reporting in main.rs only. Provides helpful error context and backtraces
- **thiserror** (2.0.12) - Derive macro for custom error types. Enables proper error handling throughout codebase

#### Hashing Dependencies
- **sha2** (0.10.8) - SHA-256 implementation. Required for cryptographically secure hashing
- **md-5** (0.10.6) - MD5 implementation. Required for backward compatibility with legacy systems
- **digest** (0.10.7) - Trait definitions for hash functions. Enables generic programming over hash algorithms

#### Performance Dependencies
- **rayon** (1.10.0) - Data parallelism for Merkle tree computation. Critical for multicore performance

#### Utility Dependencies
- **fern** (0.7.1) - Logging configuration with colors. Provides structured, readable log output
- **log** (0.4.27) - Logging facade. Standard Rust logging interface
- **jiff** (0.2.6) - Time handling for timestamps. More robust than chrono for timestamp formatting

#### Dev Dependencies (Test Only)
- **pretty_assertions** (1.4.1) - Better test assertion output. Improves debugging experience
- **tempfile** (3.19.1) - Temporary file creation for tests. Essential for file I/O testing
- **proptest** (1.4.0) - Property-based testing. Ensures algorithmic correctness
- **criterion** (0.5.1) - Benchmarking framework. Measures performance improvements
- **rstest** (0.18.2) - Parametrized testing. Reduces test boilerplate

Note: Previous questionable dependencies (displaydoc, base64ct, indicatif) have been removed

### Adding New Dependencies

Before adding any dependency:
1. Check if std library can solve the problem
2. Document why the dependency is necessary
3. Get explicit approval from the project maintainer
4. Add justification to this list
5. Ensure it's added to flake.nix

## Version Control Guidelines

### CRITICAL: Reverse-Logic .gitignore

**This project uses a REVERSE-LOGIC .gitignore pattern:**

1. **Everything is ignored by default** with `/*` at the top
2. **Specific files are explicitly allowed** with `!` prefix
3. **BE EXPLICIT** - List individual files and directories
4. **Wildcards are only acceptable for file extensions** (e.g., `*.rs`, `*.yml`)

**Why?** This prevents accidentally committing sensitive files, build artifacts, or temporary files. It forces deliberate decisions about what to track.

**Rules:**
- **NEVER** use wildcards like `/**` or `/*` in allow patterns
- **ALWAYS** list files explicitly: `!/src/main.rs` not `!/src/**`
- **ONLY** use wildcards for specific extensions: `!/tests/data/*.md5`

**To add a new file to version control:**
```gitignore
# Good - Explicit
!/src/new_module.rs
!/tests/data/specific_test.txt

# Bad - Too broad
!/src/**
!/tests/**
```

## Common Tasks

### Adding a New Hash Algorithm

1. Add variant to `HashingAlgo` enum
2. Implement parsing in `FromStr` trait
3. Add hash computation in `Hasher` methods
4. Add algorithm-specific tests
5. Update documentation and README

### Improving Performance

1. Profile with large genomics files
2. Identify bottlenecks (usually I/O or memory)
3. Consider buffer size adjustments
4. Ensure rayon is utilized effectively
5. Benchmark before and after changes

### Adding New Commands

1. Define command in `cli.rs` using clap derive
2. Implement command logic in appropriate module
3. Add comprehensive error handling
4. Include progress indicators for long operations
5. Add at least 3 tests

## Current Implementation Status

### Completed Features ✅

1. **Core Hashing Engine**
   - Merkle tree implementation with parallel processing
   - MD5 and SHA256 support with extensible design
   - Efficient 1MB chunk processing
   - Proper resource limits and assertions

2. **CLI Commands**
   - `hash` - Generate hashes for files with wildcard support
   - `verify` - Verify single file against known hash
   - `verify-many` - Batch verification from checksum file
   - Beautiful CLI with custom ANSI styling

3. **Performance Optimizations**
   - Parallel hash computation using rayon
   - Efficient buffer reuse
   - Minimal allocations in hot paths
   - Configurable thread pool

4. **Error Handling**
   - Custom error types with context
   - Helpful error messages with solutions
   - Proper error propagation
   - No unwraps in production code

### Code Quality Metrics

- **Modules**: 5 well-organized modules
- **Test Coverage**: Comprehensive tests for core functionality
- **Performance**: Merkle trees provide significant speedup on multicore
- **Documentation**: Extensive rustdoc comments with examples

### Next Steps for Expansion

1. **Additional Hash Algorithms**
   - SHA3 family for future-proofing
   - BLAKE3 for extreme performance
   - CRC32 for quick integrity checks

2. **Advanced Features**
   - Recursive directory hashing
   - Progress bars for large file operations
   - Resumable hashing for interrupted operations
   - Memory-mapped file support for huge files

3. **Integration Features**
   - JSON/CSV output formats
   - Integration with bioinformatics pipelines
   - S3/cloud storage support
   - Parallel verification of remote files

4. **Performance Enhancements**
   - SIMD optimizations where applicable
   - Custom memory allocator for hot paths
   - Zero-copy operations
   - GPU acceleration exploration

## Development Workflow

```bash
# Development
just test           # Run all tests
just check          # Run fmt, clippy, and tests
just bench          # Run benchmarks

# Building
cargo build         # Debug build
cargo build --release # Optimized build

# Quality checks (MANDATORY)
cargo fmt           # Format code
cargo clippy --all-targets --all-features -- -D warnings
```

## For AI Assistants

When helping with this project:

1. **ALWAYS** read AGENTS.md and README.md first
2. **ALWAYS** run quality checks before claiming completion
3. **ALWAYS** include 3+ tests with every change
4. **ALWAYS** apply Tiger Style principles
5. **ALWAYS** consider performance implications
6. **ALWAYS** provide helpful error messages
7. Focus on bioinformatics use cases

Remember: This tool must handle terabyte-scale genomics files efficiently.
Performance and correctness are paramount.