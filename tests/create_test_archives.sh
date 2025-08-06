#!/bin/bash
set -euo pipefail

echo "Creating test archives for checkle archive testing..."

cd "$(dirname "$0")/data"

# Create a test file with known content if it doesn't exist
if [ ! -f test_file.txt ]; then
    echo "test content for archives" > test_file.txt
fi

# Ensure we have existing test files
if [ ! -f file1.txt ]; then
    echo "Creating file1.txt for archive testing..."
    echo "This is file1 content for testing" > file1.txt
fi

echo "Creating archives in all supported formats..."

# Create archives in all supported formats
tar cf test_archive.tar test_file.txt file1.txt
tar czf test_archive.tar.gz test_file.txt file1.txt
tar cjf test_archive.tar.bz2 test_file.txt file1.txt  
tar cJf test_archive.tar.xz test_file.txt file1.txt
zip test_archive.zip test_file.txt file1.txt

echo "Creating MD5 hashes of the ARCHIVE FILES themselves..."
# Create MD5/SHA256 hashes of the ARCHIVE FILES themselves
md5sum test_archive.tar > test_archive.tar.md5
md5sum test_archive.tar.gz > test_archive.tar.gz.md5
md5sum test_archive.tar.bz2 > test_archive.tar.bz2.md5
md5sum test_archive.tar.xz > test_archive.tar.xz.md5
md5sum test_archive.zip > test_archive.zip.md5

echo "Creating SHA256 hashes of the ARCHIVE FILES themselves..."
sha256sum test_archive.tar > test_archive.tar.sha256
sha256sum test_archive.tar.gz > test_archive.tar.gz.sha256
sha256sum test_archive.tar.bz2 > test_archive.tar.bz2.sha256
sha256sum test_archive.tar.xz > test_archive.tar.xz.sha256
sha256sum test_archive.zip > test_archive.zip.sha256

echo "Test archives created successfully:"
echo "  - test_archive.tar"
echo "  - test_archive.tar.gz" 
echo "  - test_archive.tar.bz2"
echo "  - test_archive.tar.xz"
echo "  - test_archive.zip"
echo ""
echo "Hash files created for validation:"
echo "  - *.md5 files (MD5 hashes of archive files)"
echo "  - *.sha256 files (SHA256 hashes of archive files)"
echo ""
echo "These archives should be hashed as regular files by checkle unless ':' syntax is used."