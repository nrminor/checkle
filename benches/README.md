# Benchmarks

Performance benchmarks comparing checkle to standard utilities.

## Running Benchmarks

```bash
# Comprehensive benchmark suite
./benches/benchmark_checksums.sh

# Individual algorithm benchmarks
just benchmark-md5 test-file.bin
just benchmark-sha256 test-file.bin
```

## Test Files

```bash
# Create standard test files (1MB, 100MB, 1GB)
just create-test-files

# Custom sizes
dd if=/dev/urandom of=test-10gb.bin bs=1M count=10240
```

## Tools Compared

- checkle (parallel Merkle tree)
- md5sum (standard utility)
- sha256sum (standard utility)
- rhash (multi-hash tool)
- xxhsum (xxHash)
- b3sum (BLAKE3)