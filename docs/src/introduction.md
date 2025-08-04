# Introduction

Welcome to the Checkle documentation! Checkle is a fast and versatile file integrity checker designed for modern systems.

## What is Checkle?

Checkle is a command-line tool that helps you verify file integrity through various hash algorithms. It's built with performance in mind, utilizing parallel processing and SIMD acceleration where available.

## Key Features

- **Multiple Hash Algorithms**: Support for MD5, SHA-256, SHA-512, BLAKE3, and more
- **High Performance**: Parallel processing and SIMD acceleration
- **Archive Support**: Check files within archives without extraction
- **Flexible Output**: JSON, CSV, or human-readable formats
- **Cross-Platform**: Works on Linux, macOS, and Windows

## Quick Example

```bash
# Generate checksums for all files in a directory
checkle -a sha256 /path/to/directory

# Verify checksums from a file
checkle verify checksums.txt

# Check specific files with multiple algorithms
checkle -a md5,sha256,blake3 file1.txt file2.bin
```

## Getting Started

Head over to the [Installation](./guide/installation.md) guide to get started with Checkle.