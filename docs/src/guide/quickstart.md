# Quick Start

Get up and running with checkle in 5 minutes.

## Basic Commands

### Hash a single file
```bash
checkle hash genome.fastq.gz
```

### Hash multiple files
```bash
checkle hash *.fastq.gz
```

### Hash with SHA-256 instead of MD5
```bash
checkle hash --algo sha256 genome.fastq.gz
```

### Save checksums to a file
```bash
checkle hash *.fastq.gz -o checksums.txt
```

## Verification

### Verify a single file
```bash
checkle verify genome.fastq.gz --hash d41d8cd98f00b204e9800998ecf8427e
```

### Verify multiple files from a checksum file
```bash
checkle verify-many --checksum-file checksums.txt
```

## Working with Directories

### Hash all files in a directory recursively
```bash
checkle hash /data/sequencing_run --recursive
```

### Hash only specific file types
```bash
checkle hash /data --recursive --include "*.fastq" --include "*.fasta"
```

### Exclude certain patterns
```bash
checkle hash /data --recursive --exclude "*.tmp" --exclude "*.log"
```

## Archive Support

### Hash files inside a TAR archive without extracting
```bash
checkle hash data.tar.gz:sequences/sample.fastq
```

### Hash all files in an archive
```bash
checkle hash data.tar.gz:*
```

## Output Formats

### JSON output for downstream processing
```bash
checkle hash *.bam --format json > checksums.json
```

### CSV for spreadsheet import
```bash
checkle hash *.vcf --format csv > checksums.csv
```

### Pretty table display
```bash
checkle hash *.fastq --pretty
```

## Performance Tuning

### Increase parallel readers for large files
```bash
checkle hash huge_genome.fasta --parallel-readers 16
```

### Adjust chunk size for optimal performance
```bash
checkle hash *.bam --chunk-size-kb 4096
```

## Next Steps

- See [Command Line Usage](./cli.md) for detailed command reference
- Learn about [Performance](../features/performance.md) optimization
- Explore [Archive Support](../features/archives.md) features