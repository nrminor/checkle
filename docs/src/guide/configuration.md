# Configuration

Configure checkle for optimal performance in your environment.

## Performance Tuning

### Chunk Size

The chunk size determines how much data is read at once. Larger chunks can improve performance for sequential reads.

```bash
# Default: 1MB chunks
checkle hash file.txt

# Larger chunks for fast SSDs
checkle hash file.txt --chunk-size-kb 4096

# Smaller chunks for slower storage
checkle hash file.txt --chunk-size-kb 256
```

Recommendations:
- **Fast NVMe SSDs**: 4096-8192 KB
- **Standard SSDs**: 1024-2048 KB (default)
- **HDDs**: 256-512 KB
- **Network storage**: 128-256 KB

### Parallel Readers

Control how many threads read file data in parallel.

```bash
# Auto-detect (default)
checkle hash large_file.bin

# Explicit thread count
checkle hash large_file.bin --parallel-readers 8

# Single-threaded for debugging
checkle hash large_file.bin --parallel-readers 1
```

Guidelines:
- Files <64MB: Single thread (automatic)
- Files ≥64MB: Multi-threaded based on CPU cores
- Maximum useful: ~16 threads (I/O bound)

### Batch Processing

When processing many files, control parallelism:

```bash
# Process 8 files simultaneously
checkle verify-many --checksum-file list.txt --parallel-files 8

# Limit batch size for memory constraints
checkle hash /data --recursive --max-files-batch 100
```

## Algorithm Selection

Choose the right algorithm for your needs:

### MD5 (Default)
- **Speed**: Fastest
- **Security**: Not cryptographically secure
- **Use case**: Data integrity, duplicate detection
- **Compatibility**: Universal support

```bash
checkle hash file.txt --algo md5
```

### SHA-256
- **Speed**: Slower than MD5
- **Security**: Cryptographically secure
- **Use case**: Security-critical verification
- **Compatibility**: Wide support

```bash
checkle hash file.txt --algo sha256
```

## Output Configuration

### File Output
```bash
# Text format (default)
checkle hash *.txt -o checksums.txt

# JSON for programmatic use
checkle hash *.txt --format json -o checksums.json

# CSV for spreadsheets
checkle hash *.txt --format csv -o checksums.csv
```

### Display Options
```bash
# Pretty table to stderr
checkle hash *.txt --pretty

# Absolute paths
checkle hash *.txt --absolute-paths

# Per-file checksum files
checkle hash *.txt --per-file
```

## Filtering

### Include/Exclude Patterns
```bash
# Include only specific extensions
checkle hash /data --include "*.fastq" --include "*.fasta"

# Exclude temporary files
checkle hash /data --exclude "*.tmp" --exclude "*.swp"

# Ignore .gitignore rules
checkle hash /project --no-ignore
```

### Directory Traversal
```bash
# Recursive (process subdirectories)
checkle hash /data --recursive

# Non-recursive (default)
checkle hash /data
```

## Environment Variables

While checkle doesn't require environment variables, you can use shell features:

```bash
# Set default algorithm
alias checkle='checkle --algo sha256'

# Set default verbosity
export CHECKLE_OPTS='-vv'
checkle hash file.txt $CHECKLE_OPTS
```

## Memory Usage

Memory usage scales with:
- Number of parallel readers × chunk size
- Number of files processed in parallel
- Archive decompression buffers

Typical memory usage:
- Single large file: ~64MB
- Batch processing: ~256MB
- Archive processing: ~128MB per archive

To reduce memory usage:
```bash
# Smaller chunks
checkle hash large_file --chunk-size-kb 256

# Fewer parallel operations
checkle hash /data --parallel-readers 2 --max-files-batch 10
```