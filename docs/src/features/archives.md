# Archive Support

checkle can hash files within archives without extracting them.

## Supported Formats

### TAR Archives
- `.tar` - Uncompressed
- `.tar.gz` / `.tgz` - Gzip compressed
- `.tar.bz2` - Bzip2 compressed
- `.tar.xz` - XZ compressed

### ZIP Archives
- `.zip` - Various compression methods

## Basic Usage

### Hash Specific File in Archive
```bash
checkle hash archive.tar:path/to/file.txt
```

### Hash All Files in Archive
```bash
checkle hash archive.tar.gz:*
```

### Hash Files Matching Pattern
```bash
checkle hash data.zip:*.csv
checkle hash backup.tar:logs/*.log
```

## Archive Path Syntax

Use colon (`:`) to separate archive from internal path:

```
archive_path:internal_path
```

Examples:
```bash
# Specific file
data.tar.gz:results/output.txt

# All files
data.tar.gz:*

# Pattern matching
data.zip:*.fastq
data.tar:experiments/*/results.csv
```

## Pattern Matching

### Wildcards
- `*` - Match any characters (except `/`)
- `**` - Match any characters (including `/`)
- `?` - Match single character

### Examples
```bash
# All CSV files in root
checkle hash archive.zip:*.csv

# All files in subdirectory
checkle hash archive.tar:data/*

# Recursive pattern
checkle hash archive.tar.gz:**/*.txt
```

## Performance

### Streaming Processing
- Files processed without full extraction
- Memory usage bounded
- Decompression on-the-fly

### Limitations
- Sequential access within archives
- Cannot parallelize individual archive entries
- Compressed archives require decompression

## Examples

### Genomics Data
```bash
# Hash FASTQ files in compressed archive
checkle hash sequencing_run.tar.gz:*.fastq

# Verify specific sample
checkle verify reads.tar.gz:sample_001.fastq --hash abc123
```

### Backup Verification
```bash
# Hash all files in backup
checkle hash backup.tar.gz:* -o backup_checksums.txt

# Verify backup integrity later
checkle verify-many --checksum-file backup_checksums.txt
```

### Data Transfer
```bash
# Before transfer - hash archive contents
checkle hash data.tar.gz:* > checksums_before.txt

# After transfer - verify integrity
checkle hash data.tar.gz:* > checksums_after.txt
diff checksums_before.txt checksums_after.txt
```

## Archive vs Regular File

### Without colon - hash the archive itself
```bash
checkle hash archive.tar.gz
# Output: abc123def456  archive.tar.gz
```

### With colon - hash contents
```bash
checkle hash archive.tar.gz:file.txt
# Output: 789xyz012  archive.tar.gz:file.txt
```

## Compressed Archives

Compression is handled transparently:

```bash
# All work the same way
checkle hash data.tar:file.txt      # Uncompressed
checkle hash data.tar.gz:file.txt   # Gzip
checkle hash data.tar.bz2:file.txt  # Bzip2
checkle hash data.tar.xz:file.txt   # XZ
```

## Verification

### Single File in Archive
```bash
checkle verify archive.tar:important.dat --hash e3b0c44298fc1c14
```

### Multiple Files
Create checksum file:
```bash
checkle hash archive.tar:* -o archive_checksums.txt
```

Verify later:
```bash
checkle verify-many --checksum-file archive_checksums.txt
```

## Tips

1. **Use patterns to hash multiple files** - More efficient than individual commands
2. **Save checksums for archives** - Verify integrity without re-reading
3. **Compressed archives are slower** - Decompression adds overhead
4. **Large archives work fine** - Streaming prevents memory issues
5. **Archive path must exist** - Archive file itself must be accessible