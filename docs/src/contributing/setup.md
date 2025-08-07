# Development Setup

Get your development environment ready for contributing to checkle.

## Prerequisites

### Required
- Rust 1.70+ (latest stable recommended)
- Git
- C compiler (for dependencies)

### Optional
- Just (command runner)
- cargo-watch (auto-rebuild)
- cargo-criterion (benchmarking)

## Clone Repository

```bash
git clone https://github.com/nrminor/checkle.git
cd checkle
```

## Install Rust

If you don't have Rust installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

Update to latest stable:
```bash
rustup update stable
rustup default stable
```

## Install Development Tools

### Just (Command Runner)
```bash
# macOS
brew install just

# Linux
curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to ~/.local/bin

# Via cargo
cargo install just
```

### Development Dependencies
```bash
# Auto-rebuild on changes
cargo install cargo-watch

# Benchmarking
cargo install cargo-criterion

# Code coverage
cargo install cargo-tarpaulin
```

## Build and Test

### Using Just
```bash
# Run all checks
just check

# Run tests
just test

# Run clippy
just clippy

# Format code
just fmt

# Run benchmarks
just bench
```

### Using Cargo Directly
```bash
# Build debug version
cargo build

# Build release version
cargo build --release

# Run tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

## Development Workflow

### 1. Create Feature Branch
```bash
git checkout -b feature/my-feature
```

### 2. Make Changes
```bash
# Edit files
vim src/module.rs

# Auto-rebuild and test
cargo watch -x test
```

### 3. Run Checks
```bash
# Format code
cargo fmt

# Run clippy (MUST pass)
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test

# Verify hashes
just verify-hashes
```

### 4. Test Release Build
```bash
cargo build --release
./target/release/checkle hash test.txt
```

## Code Standards

### Mandatory Checks
Every change MUST pass:

1. `cargo fmt` - Code formatting
2. `cargo clippy --all-targets --all-features -- -D warnings` - Zero warnings
3. `cargo test` - All tests pass
4. `just verify-hashes` - Hash correctness

### Documentation
- All public items need rustdoc
- Include examples in doc comments
- Update relevant .md files

### Testing
- Add at least 3 tests per change
- Use proptest for algorithmic code
- Integration tests for CLI changes

## Project Structure

```
checkle/
├── src/
│   ├── main.rs           # Binary entry point
│   ├── lib.rs            # Library root
│   ├── cli.rs            # CLI definitions
│   ├── hashing.rs        # Core hashing logic
│   ├── io.rs             # File I/O
│   ├── errors.rs         # Error types
│   └── commands/         # Command implementations
├── tests/                # Integration tests
├── benches/              # Benchmarks
├── docs/                 # mdBook documentation
└── justfile              # Development commands
```

## Common Tasks

### Add New Hash Algorithm
1. Add variant to `HashingAlgo` enum
2. Implement in `Hasher` methods
3. Add tests
4. Update documentation

### Add New Command
1. Define in `cli.rs`
2. Implement in `commands/`
3. Add integration tests
4. Update CLI documentation

### Improve Performance
1. Run benchmarks: `just bench`
2. Profile with `cargo flamegraph`
3. Make changes
4. Verify with benchmarks
5. Ensure tests still pass

## Debugging

### Enable Debug Output
```bash
RUST_LOG=debug cargo run -- hash test.txt
```

### Run with Backtrace
```bash
RUST_BACKTRACE=1 cargo run -- hash test.txt
```

### Use GDB/LLDB
```bash
cargo build
gdb target/debug/checkle
```

## IDE Setup

### VS Code
Install extensions:
- rust-analyzer
- CodeLLDB (debugging)

### IntelliJ/CLion
Install Rust plugin

### Vim/Neovim
Use rust.vim and rust-analyzer LSP

## Getting Help

- Check existing issues on GitHub
- Read AGENTS.md for AI assistant guidelines
- Ask in discussions/issues
- Review test files for examples