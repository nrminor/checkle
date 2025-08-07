# Building from Source

Build checkle from source code for development or custom installations.

## Quick Build

### Standard Build
```bash
git clone https://github.com/nrminor/checkle.git
cd checkle
cargo build --release
```

Binary will be at: `target/release/checkle`

### SIMD-Optimized Build
```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release --features simd
```

## Build Types

### Debug Build
Fast compilation, slow runtime, debug symbols:
```bash
cargo build
# Binary at: target/debug/checkle
```

### Release Build
Slow compilation, fast runtime, optimized:
```bash
cargo build --release
# Binary at: target/release/checkle
```

### SIMD Build
Hardware-specific optimizations:
```bash
# For current CPU
RUSTFLAGS="-C target-cpu=native" cargo build --release --features simd

# For specific architecture
RUSTFLAGS="-C target-feature=+avx2" cargo build --release --features simd
```

## Installation

### System-Wide
```bash
# Build and install to ~/.cargo/bin
cargo install --path .

# Or copy manually
sudo cp target/release/checkle /usr/local/bin/
```

### Local Directory
```bash
# Build and copy to specific location
cargo build --release
cp target/release/checkle ~/bin/
```

## Cross-Compilation

### Linux to Windows
```bash
# Install target
rustup target add x86_64-pc-windows-gnu

# Build
cargo build --release --target x86_64-pc-windows-gnu
```

### Linux to macOS
```bash
# Install target
rustup target add x86_64-apple-darwin

# Build (requires macOS SDK)
cargo build --release --target x86_64-apple-darwin
```

### Using Cross
```bash
# Install cross
cargo install cross

# Build for target
cross build --release --target aarch64-unknown-linux-gnu
```

## Build Features

### Available Features
- `simd` - Enable SIMD optimizations (requires nightly)

### Feature Combinations
```bash
# Default (no features)
cargo build --release

# With SIMD
cargo build --release --features simd

# All features
cargo build --release --all-features
```

## Build Requirements

### Minimum Requirements
- Rust 1.70+
- 2GB RAM
- 500MB disk space

### Recommended
- Rust latest stable
- 4GB RAM
- 1GB disk space
- SSD for faster builds

## Platform-Specific Notes

### Linux
No special requirements. Works on all major distributions.

### macOS
```bash
# Install Xcode command line tools if needed
xcode-select --install
```

### Windows
Use either:
- MSVC toolchain (Visual Studio)
- GNU toolchain (MinGW)

```bash
# MSVC
rustup default stable-msvc

# GNU
rustup default stable-gnu
```

## Optimizations

### Link-Time Optimization (LTO)
Add to `Cargo.toml`:
```toml
[profile.release]
lto = true
```

### Codegen Units
For maximum performance:
```toml
[profile.release]
codegen-units = 1
```

### CPU-Specific
```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

## Docker Build

```dockerfile
FROM rust:latest as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/checkle /usr/local/bin/
CMD ["checkle"]
```

Build:
```bash
docker build -t checkle .
```

## Nix Build

Using flake:
```bash
nix build
```

Traditional:
```bash
nix-build
```

## Verification

After building, verify the binary works:

```bash
# Check version
./target/release/checkle --version

# Run tests
cargo test

# Verify against standard tools
echo "test" > test.txt
./target/release/checkle hash test.txt
md5sum test.txt  # Should match
```

## Troubleshooting

### Out of Memory
```bash
# Limit parallel jobs
cargo build --release -j 2
```

### Linker Errors
```bash
# On Linux, install development packages
sudo apt-get install build-essential
# or
sudo yum groupinstall "Development Tools"
```

### SIMD Build Fails
```bash
# Use nightly Rust
rustup install nightly
cargo +nightly build --release --features simd
```

### Slow Builds
```bash
# Use sccache
cargo install sccache
export RUSTC_WRAPPER=sccache
cargo build --release
```

## Build Artifacts

Build produces:
```
target/
├── release/
│   ├── checkle          # Main binary
│   ├── deps/            # Dependencies
│   └── build/           # Build scripts output
└── debug/               # Debug build (if built)
```

## Clean Build

```bash
# Remove all build artifacts
cargo clean

# Remove only release artifacts
cargo clean --release
```