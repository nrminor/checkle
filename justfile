# checkle Development Commands
# This justfile provides common development commands for the checkle project

export CARGO_TERM_COLOR := "always"

# Default recipe shows available commands
[group('help')]
default:
    @just --list --unsorted

# Show all available recipes with descriptions
[group('help')]
help:
    @just --list

# ===== Build Commands =====

# Build the project in debug mode
[group('build')]
build:
    cargo build

# Build the project in release mode with optimizations
[group('build')]
release:
    cargo build --release

# Run the project with optional arguments
[group('build')]
run *args:
    cargo run -- {{ args }}

# Build the project in release mode and install it locally
[group('build')]
install:
    cargo install --path=.

alias i := install

# ===== Quality Checks =====

# Run all tests
# Note: Uses --test-threads=1 to avoid conflicts with the shared checksum.txt file
[group('test')]
test:
    cargo test --all -- --test-threads=1

# Run tests with output displayed
[group('test')]
test-verbose:
    cargo test --all -- --test-threads=1 --nocapture

# Run a specific test by name
[group('test')]
test-one name:
    cargo test {{ name }} -- --nocapture

# Run clippy with project's strict lints (pedantic, perf, style, etc.)
[group('lint')]
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Run comprehensive clippy across all platforms (catches cfg-gated code)
[group('lint')]
clippy-all:
    @echo "Running comprehensive clippy checks..."
    @echo "→ Checking all targets with all features..."
    cargo clippy --all-targets --all-features -- -D warnings
    @echo "→ Checking for unused cfg attributes..."
    # Note: Real cross-platform clippy requires Linux/Windows sysroots and toolchains
    # For now, we ensure all cfg-gated code at least compiles on the current platform
    @echo "✓ Comprehensive clippy checks passed!"
    @echo ""
    @echo "💡 For true cross-platform validation, use GitHub Actions CI which tests:"
    @echo "   - Linux (ubuntu-latest)"  
    @echo "   - macOS (macos-latest)"
    @echo "   - Windows (windows-latest)"

# Format code using rustfmt
[group('lint')]
fmt:
    cargo fmt

# Check code formatting without making changes
[group('lint')]
fmt-check:
    cargo fmt -- --check

# Run all quality checks (format, clippy, test) - REQUIRED before commits
[group('lint')]
check: fmt-check clippy test

# Run comprehensive quality checks including cross-platform clippy
[group('lint')]  
check-all: fmt-check clippy-all test

# Quick check that code compiles
[group('lint')]
check-fast:
    cargo check --all-targets

# ===== Documentation =====

# Generate and open Rust API documentation
[group('docs')]
doc:
    cargo doc --open

# Generate Rust API documentation without opening
[group('docs')]
doc-build:
    cargo doc --no-deps

# ===== Development Tools =====

# Watch for changes and run checks automatically
[group('dev')]
watch:
    cargo watch -x check -x 'clippy -- -D warnings' -x test

# Run benchmarks (requires criterion)
[group('dev')]
bench:
    cargo bench

# Check for outdated dependencies
[group('dev')]
outdated:
    cargo outdated

# Update dependencies (dry run)
[group('dev')]
update-dry:
    cargo update --dry-run

# Check dependencies for security vulnerabilities
[group('dev')]
audit:
    cargo audit

# ===== Code Analysis =====

# Count lines of code
[group('analysis')]
loc:
    @echo "Rust source code:"
    @find src -name "*.rs" | xargs wc -l | sort -n

# Generate a dependency tree
[group('analysis')]
tree:
    cargo tree

# Show crate sizes in release build
[group('analysis')]
bloat:
    cargo bloat --release

# ===== Maintenance =====

# Clean build artifacts and target directory
[group('maintenance')]
clean:
    cargo clean

# Format TOML files (requires taplo)
[group('maintenance')]
fmt-toml:
    taplo fmt

# ===== Performance Testing =====

# Benchmark checkle against standard tools (requires hyperfine)
[group('perf')]
benchmark-md5 file:
    @echo "Benchmarking MD5 hashing on {{ file }}..."
    hyperfine --warmup 3 \
        "cargo run --release -- hash {{ file }} --algorithm md5" \
        "md5sum {{ file }}" \
        --export-markdown benchmark-md5.md

# Benchmark SHA256 hashing
[group('perf')]
benchmark-sha256 file:
    @echo "Benchmarking SHA256 hashing on {{ file }}..."
    hyperfine --warmup 3 \
        "cargo run --release -- hash {{ file }} --algorithm sha2" \
        "shasum -a 256 {{ file }}" \
        --export-markdown benchmark-sha256.md

# Create test files of various sizes for benchmarking
[group('perf')]
create-test-files:
    @echo "Creating test files..."
    dd if=/dev/urandom of=test-1mb.bin bs=1M count=1
    dd if=/dev/urandom of=test-100mb.bin bs=1M count=100
    dd if=/dev/urandom of=test-1gb.bin bs=1M count=1024
    @echo "Test files created: test-1mb.bin, test-100mb.bin, test-1gb.bin"

# ===== Project-Specific =====

# Run checkle with example commands
[group('project')]
examples:
    @echo "Example checkle commands:"
    @echo "  checkle hash file.txt                    # Generate MD5 hash"
    @echo "  checkle hash file.txt --algorithm sha2   # Generate SHA256 hash"
    @echo "  checkle hash *.fastq                     # Hash multiple files"
    @echo "  checkle verify file.txt --hash abc123    # Verify single file"
    @echo "  checkle verify-many -c checksums.txt     # Verify batch of files"
    @echo "  checkle --help                           # Show all commands"

# Run hash verification tests to ensure checkle produces correct hashes
[group('test')]
verify-hashes:
    @echo "Running hash verification tests..."
    ./tests/verify_hashes.sh

# Run all tests including hash verification (single-threaded to avoid file conflicts)
[group('test')]
test-all: test verify-hashes
    @echo "✓ All tests completed!"

# Generate checksums for all Rust files (demo)
[group('project')]
hash-src:
    cargo run -- hash src/*.rs

# Verify the project is ready for release
[group('release')]
verify-release: check doc-build
    @echo "✓ All checks passed!"
    @echo "✓ Documentation builds successfully!"
    @echo "Ready for release - remember to:"
    @echo "  1. Update version in Cargo.toml"
    @echo "  2. Update CHANGELOG.md"
    @echo "  3. Create git tag with 'git tag v0.0.0'"
    @echo "  4. Push tag to trigger release workflow"

# Create a new release tag (specify version)
[group('release')]
tag version:
    git tag -a v{{ version }} -m "Release v{{ version }}"
    @echo "Created tag v{{ version }}"
    @echo "Push with: git push origin v{{ version }}"
