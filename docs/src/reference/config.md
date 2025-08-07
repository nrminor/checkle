# Configuration Reference

Detailed configuration options for checkle.

## Performance Parameters

### chunk-size-kb

Controls the size of data chunks read from files.

- **Type**: Integer
- **Default**: 1024 (1MB)
- **Range**: 4 - 65536
- **Unit**: Kilobytes

**Impact**:
- Larger chunks: Better sequential throughput, more memory
- Smaller chunks: Less memory, better for slow storage

**Recommendations**:
| Storage Type | Recommended Size |
|--------------|------------------|
| NVMe SSD | 4096-8192 KB |
| SATA SSD | 1024-2048 KB |
| HDD | 256-512 KB |
| Network | 128-256 KB |

### parallel-readers

Number of threads for parallel file reading.

- **Type**: Integer
- **Default**: Auto (based on CPU cores)
- **Range**: 1 - 64
- **Auto logic**: min(CPU_cores, 8)

**Impact**:
- More threads: Better parallelization for large files
- Fewer threads: Less memory usage, less I/O contention

**Guidelines**:
- Files <64MB: Always single-threaded
- Files ≥64MB: Multi-threaded based on size
- I/O bound after ~16 threads

### max-files-batch

Maximum files to process simultaneously.

- **Type**: Integer
- **Default**: 1000
- **Range**: 1 - 10000

**Impact**:
- Larger batch: Better throughput for many small files
- Smaller batch: Less memory usage

### parallel-files

Files to verify in parallel (verify-many only).

- **Type**: Integer
- **Default**: 4
- **Range**: 1 - 32

**Impact**:
- More parallel: Faster verification of many files
- Less parallel: Lower system load

## Algorithm Configuration

### algo

Hash algorithm selection.

- **Type**: Enum
- **Values**: md5, sha256
- **Default**: md5

**Characteristics**:

| Algorithm | Speed | Security | Size | Compatibility |
|-----------|-------|----------|------|---------------|
| md5 | Fastest | Weak | 128-bit | Universal |
| sha256 | Moderate | Strong | 256-bit | Wide |

## Output Configuration

### format

Output format for checksums.

- **Type**: Enum
- **Values**: text, json, csv
- **Default**: text (auto-detected from file extension)

**Auto-detection**:
- `.json` → JSON format
- `.csv` → CSV format
- Others → text format

### pretty

Display results in formatted table.

- **Type**: Boolean
- **Default**: false
- **Output**: stderr (doesn't interfere with stdout)

### per-file

Create individual checksum files.

- **Type**: Boolean
- **Default**: false
- **Naming**: `<filename>.<algo>` (e.g., `file.txt.md5`)

### absolute-paths

Use absolute paths in output.

- **Type**: Boolean
- **Default**: false (relative paths)

## Filtering Configuration

### include

Patterns for files to include.

- **Type**: String (multiple allowed)
- **Syntax**: Glob patterns
- **Default**: Include all

**Examples**:
```bash
--include "*.txt"
--include "data/*.csv"
--include "**/*.fastq"
```

### exclude

Patterns for files to exclude.

- **Type**: String (multiple allowed)
- **Syntax**: Glob patterns
- **Default**: Exclude none

**Examples**:
```bash
--exclude "*.tmp"
--exclude ".git/**"
--exclude "**/test/*"
```

### no-ignore

Don't respect .gitignore rules.

- **Type**: Boolean
- **Default**: false (respect .gitignore)

### recursive

Process directories recursively.

- **Type**: Boolean
- **Default**: false

## Progress Configuration

### no-progress

Disable progress bars.

- **Type**: Boolean
- **Default**: false (show progress)

**Auto-disabled when**:
- Output is not a terminal
- Quiet mode is enabled
- Single small file

## Verification Configuration

### fail-fast

Stop on first verification failure.

- **Type**: Boolean
- **Default**: false (verify all)
- **Applies to**: verify-many

### failed-only

Show only failed verifications.

- **Type**: Boolean
- **Default**: false (show all)
- **Applies to**: verify-many

### quiet

Suppress non-error output.

- **Type**: Boolean
- **Default**: false
- **Applies to**: verify-many

## Resource Limits

Built-in limits for stability:

| Resource | Limit | Configurable |
|----------|-------|--------------|
| Max chunk size | 64MB | Yes (chunk-size-kb) |
| Max parallel readers | 64 | Yes (parallel-readers) |
| Max files batch | 10000 | Yes (max-files-batch) |
| Max archive size | 1TB | No |
| Max path length | 4096 | No |

## Default Behavior

Without options, checkle:
1. Uses MD5 algorithm
2. Auto-detects optimal thread count
3. Uses 1MB chunks
4. Shows progress bars
5. Outputs to stdout in text format
6. Processes single files/directories non-recursively
7. Respects .gitignore rules