# Performance

checkle is designed for maximum performance on modern multicore systems.

## Key Performance Features

### Parallel Processing
- Merkle tree-based parallelization
- Near-linear scaling with CPU cores
- Automatic thread count detection
- Memory-bounded operation

### Optimizations
- SIMD acceleration (optional builds)
- Zero-copy I/O where possible
- Buffer pooling to reduce allocations
- Optimized for SSD characteristics

## Benchmarks

### Single File Performance

Testing with a 10GB file on 8-core system:

| Tool | Algorithm | Time | Speed |
|------|-----------|------|-------|
| checkle | MD5 | 2.8s | 3.5 GB/s |
| md5sum | MD5 | 20s | 500 MB/s |
| checkle | SHA-256 | 4.7s | 2.1 GB/s |
| sha256sum | SHA-256 | 33s | 300 MB/s |

### Batch Processing

Processing 1000 files (100MB each):

| Tool | Time | Files/sec |
|------|------|-----------|
| checkle | 45s | 22 |
| md5sum | 200s | 5 |
| sha256sum | 330s | 3 |

## Performance Tuning

### For Large Files (>1GB)

```bash
# Increase chunk size
checkle hash large_genome.fasta --chunk-size-kb 4096

# More parallel readers
checkle hash large_genome.fasta --parallel-readers 16
```

### For Many Small Files

```bash
# Increase batch parallelism
checkle hash /data --recursive --max-files-batch 50

# Reduce per-file overhead
checkle hash /data --recursive --no-progress
```

### For Network Storage

```bash
# Smaller chunks to reduce latency impact
checkle hash /nfs/data/file.bin --chunk-size-kb 256

# Fewer parallel readers to avoid congestion
checkle hash /nfs/data/file.bin --parallel-readers 4
```

## Memory Usage

Memory scales with:
- `chunk_size × parallel_readers` per file
- Number of files in parallel batch
- Archive decompression buffers

Typical usage:
- Large file (8 threads): ~64MB
- Batch processing: ~256MB
- Archive processing: ~128MB

## CPU Utilization

checkle efficiently uses available CPU cores:

### Small Files (<64MB)
- Single-threaded (overhead not worth parallelization)
- Multiple files processed in parallel

### Large Files (≥64MB)
- Multi-threaded per file
- Scales to available cores
- I/O and CPU overlapped

## Storage Considerations

### SSD Optimization
- Default 1MB chunks align with SSD erase blocks
- Sequential reads within regions
- Minimal random I/O

### HDD Optimization
```bash
# Larger sequential reads
checkle hash /hdd/file.bin --chunk-size-kb 4096

# Single reader to avoid seek overhead
checkle hash /hdd/file.bin --parallel-readers 1
```

## SIMD Acceleration

SIMD builds provide additional speedup:

### Performance Gains
- Hex encoding: 2-3x faster
- Memory operations: 20-30% faster
- Overall: 10-15% improvement

### Using SIMD Build
```bash
# Install SIMD version
curl -fsSL https://raw.githubusercontent.com/nrminor/checkle/main/INSTALL.sh | sh -s -- --simd

# Verify SIMD support
checkle --version  # Shows "simd" in version string
```

## Comparison with Other Tools

### vs Traditional Tools (md5sum, sha256sum)
- 5-10x faster on multicore systems
- Linear scaling with cores
- Better memory efficiency

### vs Parallel Implementations
- Comparable raw performance
- Better progress reporting
- Archive support without extraction
- More output formats

## Best Practices

1. **Let checkle auto-detect settings** - Default heuristics work well
2. **Use SIMD builds on modern CPUs** - Free 10-15% speedup
3. **Match chunk size to storage** - Larger for SSD, smaller for HDD
4. **Process files in batches** - Better than one at a time
5. **Use appropriate algorithm** - MD5 for speed, SHA-256 for security