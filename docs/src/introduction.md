# Introduction

> ⚠️ **CRITICAL: DO NOT USE FOR STANDARD CHECKSUMS**
> 
> This project is an unsuccessful prototype that will produce different hashes
> than `md5sum` and `sha256sum` for all files larger than 1MB. `checkle` is thus
> incompatible with standard MD5/SHA256 checksum utilities.
> 
> Please use standard time-tested tools like `md5sum` or `sha256sum` instead.

Welcome to checkle - an extremely fast checksum utility designed for bioinformatics workflows involving terabyte-scale genomics data.

## What is checkle?

checkle is a high-performance command-line tool that leverages Merkle tree parallelization to compute checksums faster than traditional tools like md5sum or sha256sum. It's specifically optimized for bioinformatics workflows where data integrity is critical and files can be hundreds of gigabytes each.

## Key Features

- **Blazing Fast**: 5-10x faster than md5sum on multicore systems
- **Merkle Tree Parallelization**: Near-linear speedup with CPU cores
- **Archive Support**: Hash files within TAR/ZIP archives without extraction
- **Bioinformatics Focus**: Optimized for large genomics files (FASTQ, BAM, VCF)
- **Multiple Output Formats**: Text, JSON, CSV for pipeline integration
- **Progress Tracking**: Real-time progress for long-running operations

## Quick Example

```bash
# Hash a single genome file
checkle hash genome.fastq.gz

# Hash all FASTQ files in a sequencing run
checkle hash /data/run_001 --recursive --include "*.fastq.gz"

# Verify downloaded reference genome
checkle verify GRCh38.fa.gz --hash e3b0c44298fc1c149afbf4c8996fb924

# Hash files in compressed archive without extracting
checkle hash sequencing_data.tar.gz:*.fastq
```

## Getting Started

Head over to the [Installation](./guide/installation.md) guide to get started with Checkle.