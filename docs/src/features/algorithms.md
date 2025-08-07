# Hash Algorithms

checkle supports multiple hash algorithms for different use cases.

## Available Algorithms

### MD5
- **Speed**: ~500 MB/s per core
- **Hash size**: 128 bits (32 hex characters)
- **Security**: Broken for cryptographic use
- **Best for**: Fast integrity checks, duplicate detection

```bash
checkle hash file.txt --algo md5
```

### SHA-256
- **Speed**: ~300 MB/s per core
- **Hash size**: 256 bits (64 hex characters)
- **Security**: Cryptographically secure
- **Best for**: Security-critical verification, compliance

```bash
checkle hash file.txt --algo sha256
```

## Algorithm Comparison

| Algorithm | Speed | Security | Hash Size | Use Case |
|-----------|-------|----------|-----------|----------|
| MD5 | Fastest | Weak | 128 bits | Data integrity |
| SHA-256 | Moderate | Strong | 256 bits | Security verification |

## Choosing an Algorithm

### Use MD5 when:
- Speed is critical
- Processing terabyte-scale datasets
- Checking data integrity (not security)
- Compatibility with legacy systems
- Detecting accidental corruption

### Use SHA-256 when:
- Security is important
- Regulatory compliance required
- Verifying downloaded files
- Long-term archival storage
- Protecting against tampering

## Performance Characteristics

### Merkle Tree Parallelization

checkle uses Merkle trees to parallelize hashing:

1. File divided into chunks
2. Each chunk hashed independently
3. Hash results combined in binary tree
4. Single root hash produced

This provides:
- Near-linear speedup with CPU cores
- Deterministic results
- Memory-bounded operation

### Real-World Performance

On a modern 8-core system:

**MD5:**
- Single-threaded: ~500 MB/s
- Multi-threaded: ~3.5 GB/s

**SHA-256:**
- Single-threaded: ~300 MB/s
- Multi-threaded: ~2.1 GB/s

## Implementation Details

### Chunk Processing
```bash
# Default 1MB chunks
checkle hash genome.fasta

# Larger chunks for better throughput
checkle hash genome.fasta --chunk-size-kb 4096
```

### Parallel Readers
```bash
# Auto-detect optimal threads
checkle hash large_file.bin

# Manual thread control
checkle hash large_file.bin --parallel-readers 16
```

## Verification

### Single File
```bash
# MD5 verification
checkle verify file.txt --hash d41d8cd98f00b204e9800998ecf8427e

# SHA-256 verification
checkle verify file.txt --hash e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 --algo sha256
```

### Batch Verification
```bash
# From checksum file
checkle verify-many --checksum-file md5sums.txt --algo md5
```

## Compatibility

checkle produces checksums compatible with standard tools:

```bash
# checkle output matches:
md5sum file.txt
sha256sum file.txt

# Verify with standard tools:
md5sum -c checksums.md5
sha256sum -c checksums.sha256
```