# CLI Reference

Complete command-line interface reference for checkle.

## Synopsis

```
checkle [OPTIONS] <COMMAND>
```

## Global Options

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Increase verbosity (use multiple times) |
| `--quiet` | `-q` | Suppress non-essential output |
| `--version` | | Display version |
| `--help` | `-h` | Show help |

## Commands

### checkle hash

Generate checksums for files.

```
checkle hash [OPTIONS] <FILE_OR_DIR>
```

#### Arguments
- `<FILE_OR_DIR>` - File, directory, or archive path to hash

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--algo <ALGORITHM>` | md5 | Hash algorithm (md5, sha256) |
| `--recursive` | false | Process directories recursively |
| `--output <FILE>` | - | Output file for checksums |
| `--format <FORMAT>` | text | Output format (text, json, csv) |
| `--pretty` | false | Display formatted table |
| `--per-file` | false | Create individual checksum files |
| `--include <PATTERN>` | - | Include only matching files |
| `--exclude <PATTERN>` | - | Exclude matching files |
| `--no-ignore` | false | Don't respect .gitignore |
| `--parallel-readers <N>` | auto | Parallel reader threads |
| `--chunk-size-kb <SIZE>` | 1024 | Chunk size in KB |
| `--max-files-batch <N>` | 1000 | Maximum files per batch |
| `--absolute-paths` | false | Use absolute paths |
| `--no-progress` | false | Disable progress bars |

### checkle verify

Verify file against known hash.

```
checkle verify [OPTIONS] <FILE> --hash <HASH>
```

#### Arguments
- `<FILE>` - File to verify

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--hash <HASH>` | required | Expected hash value |
| `--algo <ALGORITHM>` | md5 | Hash algorithm |
| `--chunk-size-kb <SIZE>` | 1024 | Chunk size in KB |
| `--parallel-readers <N>` | auto | Parallel reader threads |
| `--no-progress` | false | Disable progress bar |

### checkle verify-many

Verify multiple files from checksum file.

```
checkle verify-many [OPTIONS] --checksum-file <FILE>
```

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--checksum-file <FILE>` | required | File containing checksums |
| `--base-dir <DIR>` | . | Base directory for paths |
| `--algo <ALGORITHM>` | md5 | Hash algorithm |
| `--fail-fast` | false | Stop on first failure |
| `--quiet` | false | Only show failures |
| `--pretty` | false | Display formatted table |
| `--parallel-files <N>` | 4 | Files to verify in parallel |
| `--chunk-size-kb <SIZE>` | 1024 | Chunk size in KB |
| `--failed-only` | false | Show only failures |
| `--no-progress` | false | Disable progress bars |

## Archive Path Syntax

```
archive.tar:internal/path/file.txt
```

Special patterns:
- `:*` - All files in archive
- `:*.ext` - Files matching pattern
- `:dir/*` - All files in directory

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Verification failure |
| 3 | File not found |
| 4 | Invalid arguments |
| 5 | I/O error |

## Environment Variables

No environment variables are used directly. Shell aliases can customize defaults:

```bash
alias checkle='checkle --algo sha256'
```

## File Formats

### Checksum File Format (text)
```
<hash>  <filepath>
```

Example:
```
d41d8cd98f00b204e9800998ecf8427e  file1.txt
e3b0c44298fc1c149afbf4c8996fb924  file2.txt
```

### JSON Format
```json
[
  {
    "hash": "d41d8cd98f00b204e9800998ecf8427e",
    "filepath": "file1.txt"
  }
]
```

### CSV Format
```csv
hash,filepath
d41d8cd98f00b204e9800998ecf8427e,file1.txt
```