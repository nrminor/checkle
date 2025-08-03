# Examples

Usage examples for checkle.

## Basic Usage

```bash
# Check single file with MD5
checkle -a md5 file.txt

# Check multiple files with SHA256
checkle -a sha256 file1.txt file2.txt

# Verify against manifest
checkle -v manifest.csv data/
```

## Output Formats

```bash
# JSON output
checkle -o json -a sha256 *.txt

# CSV output
checkle -o csv -a md5 *.bin
```

## Archive Support

```bash
# Check files inside archives (coming soon)
checkle archive.tar.gz
checkle data.zip
```