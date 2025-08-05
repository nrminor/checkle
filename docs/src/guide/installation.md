# Installation

checkle offers multiple installation methods to suit different preferences and use cases.

## Quick Install (Recommended)

The quickest way to get started is using our installation script:

```bash
# Standard build
curl -fsSL https://raw.githubusercontent.com/nrminor/checkle/main/INSTALL.sh | sh

# SIMD-optimized build (faster, requires modern CPU)
curl -fsSL https://raw.githubusercontent.com/nrminor/checkle/main/INSTALL.sh | sh -s -- --simd
```

## Manual Binary Download

Download precompiled binaries from [releases](https://github.com/nrminor/checkle/releases):

```bash
# SIMD-optimized (recommended for modern CPUs)
wget https://github.com/nrminor/checkle/releases/latest/download/checkle-x86_64-unknown-linux-gnu-simd.tar.gz
tar -xzf checkle-x86_64-unknown-linux-gnu-simd.tar.gz
sudo mv checkle /usr/local/bin/

# Standard compatibility version
wget https://github.com/nrminor/checkle/releases/latest/download/checkle-x86_64-unknown-linux-gnu.tar.gz
```

## Cargo Install

If you have Rust installed, you can build and install checkle using Cargo:

```bash
# From crates.io (when published)
cargo install checkle

# With cargo-binstall (if available)
cargo binstall checkle

# From source
cargo install --git https://github.com/nrminor/checkle
```

## Verification

After installation, verify that checkle is working correctly:

```bash
checkle --version
```

You should see output showing the installed version of checkle.

## SIMD vs Standard Builds

checkle offers two build variants:

- **SIMD-optimized**: Faster performance using advanced CPU instructions. Requires modern CPUs (x86_64 with SSE4.2+ or ARM64 with NEON).
- **Standard**: Maximum compatibility across different hardware platforms.

For best performance on modern systems, use the SIMD-optimized build. If you encounter issues or are using older hardware, use the standard build.